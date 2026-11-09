// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeIdiomatic.scala
//! Matches on an int or a string the pattern matcher leaves, as `tableswitch` or `lookupswitch`
//! (dotty's `genMatchTo`, `emitSWITCH`).

use super::body::{Body, Dest};
use super::locals::int_like;
use super::{Gen, unsupported};
use classfile::{Invoke, Label};
use lower::Result;
use std::collections::BTreeMap;
use syntax::ast::constant::Constant;
use typer::tpd::{List, TNodeId, Tree};

/// The least share of keys in the key range for a `tableswitch`.
const MIN_SWITCH_DENSITY: f64 = 0.7;

/// A case of a string match: its body, or the label of a body shared by alternatives.
enum Target {
    Body(TNodeId),
    Shared(Label),
}

impl Gen<'_, '_> {
    /// Match `t`; its value of `slots` slots goes to `dest`, given one, else stays on the stack.
    pub fn gen_match(
        &mut self,
        b: &mut Body,
        t: TNodeId,
        selector: TNodeId,
        cases: List,
        to: Option<(i32, Dest)>,
    ) -> Result<i32> {
        let post = b.code.new_label();
        let (slots, dest) = match to {
            Some(to) => to,
            None => (classfile::value_slots(&self.desc_of(t)), Dest::Jump(post)),
        };
        let depth = b.code.stack();
        let mut keys = Vec::new();
        let mut blocks: Vec<(Label, Option<TNodeId>)> = Vec::new();
        let mut default = None;
        let mut by_hash: BTreeMap<i32, Vec<(Vec<u16>, Target)>> = BTreeMap::new();
        let string = self.desc_of(selector) == "Ljava/lang/String;";
        for c in self.l.tpd.items(cases).to_vec() {
            let Tree::CaseDef { pat, guard: None, body } = *self.l.tpd.tree(c) else {
                return unsupported(self.l.tpd.span(c), "a switch case with a guard");
            };
            let label = b.code.new_label();
            let alts = match self.l.tpd.tree(pat) {
                _ if self.is_wildcard(pat) => {
                    default = Some(label);
                    blocks.push((label, Some(body)));
                    continue;
                }
                &Tree::Alternative { trees } => self.l.tpd.items(trees).to_vec(),
                _ => vec![pat],
            };
            if !string || alts.len() > 1 {
                blocks.push((label, Some(body)));
            }
            for alt in alts.iter().copied() {
                match (self.l.tpd.tree(alt), string) {
                    (Tree::Literal { value: Constant::String(s) }, true) => {
                        let s = s.to_vec();
                        let target = if alts.len() > 1 { Target::Shared(label) } else { Target::Body(body) };
                        by_hash.entry(hash(&s)).or_default().insert(0, (s, target));
                    }
                    (Tree::Literal { value }, false) if let Some(k) = int_like(value) => {
                        keys.push((k, label))
                    }
                    _ => return unsupported(self.l.tpd.span(alt), "a switch case other than a literal"),
                }
            }
        }
        let default = default.unwrap_or_else(|| {
            let label = b.code.new_label();
            blocks.push((label, None));
            label
        });
        if string {
            self.string_switch(b, selector, default, by_hash, (slots, dest))?;
        } else {
            self.expr_as(b, selector, 1)?;
            emit_switch(b, &mut keys, default);
        }
        for (label, body) in blocks {
            b.code.place(label);
            b.code.set_stack(depth);
            match body {
                Some(body) => self.expr_to(b, body, slots, dest)?,
                None => self.throw_match_error(b),
            }
        }
        if to.is_none() {
            b.code.place(post);
            b.code.set_stack(depth + slots);
        }
        Ok(if to.is_none() { slots } else { 0 })
    }

    /// The switch on the selector's hash, 0 for `null`, then the `equals` tests of each hash's
    /// strings.
    fn string_switch(
        &mut self,
        b: &mut Body,
        selector: TNodeId,
        default: Label,
        by_hash: BTreeMap<i32, Vec<(Vec<u16>, Target)>>,
        (slots, dest): (i32, Dest),
    ) -> Result<()> {
        let depth = b.code.stack();
        let (other, hashed) = (b.code.new_label(), b.code.new_label());
        self.expr_as(b, selector, 1)?;
        b.code.jump(0xc7, other);
        b.code.int(b.pool, 0);
        b.code.jump(0xa7, hashed);
        b.code.place(other);
        b.code.set_stack(depth);
        self.expr_as(b, selector, 1)?;
        b.code.invoke(Invoke::Virtual, b.pool, "java/lang/String", "hashCode", "()I");
        b.code.place(hashed);
        let blocks: Vec<(i32, Label)> = by_hash.keys().map(|&h| (h, b.code.new_label())).collect();
        let mut keys = blocks.clone();
        emit_switch(b, &mut keys, default);
        for ((_, label), (_, cases)) in blocks.into_iter().zip(by_hash) {
            b.code.place(label);
            b.code.set_stack(depth);
            for (s, target) in cases {
                let next = b.code.new_label();
                let index = b.pool.string(&s);
                self.line(b, selector);
                b.code.ldc(index);
                self.expr_as(b, selector, 1)?;
                let equals = "(Ljava/lang/Object;)Z";
                b.code.invoke(Invoke::Virtual, b.pool, "java/lang/Object", "equals", equals);
                match target {
                    Target::Shared(label) => b.code.jump(0x9a, label),
                    Target::Body(body) => {
                        b.code.jump(0x99, next);
                        self.expr_to(b, body, slots, dest)?;
                    }
                }
                b.code.place(next);
                b.code.set_stack(depth);
            }
            b.code.jump(0xa7, default);
        }
        Ok(())
    }

    /// `throw new MatchError(null)` (dotty's `emitThrowMatchError`).
    fn throw_match_error(&mut self, b: &mut Body) {
        b.code.new_object(b.pool, "scala/MatchError");
        b.code.dup();
        b.code.aconst_null();
        b.code.invoke(Invoke::Special, b.pool, "scala/MatchError", "<init>", "(Ljava/lang/Object;)V");
        b.code.insn(0xbf);
    }
}

/// A `tableswitch` if the keys are dense enough, else a `lookupswitch` (dotty's `emitSWITCH`).
fn emit_switch(b: &mut Body, keys: &mut [(i32, Label)], default: Label) {
    keys.sort_by_key(|k| k.0);
    let (Some(&(min, _)), Some(&(max, _))) = (keys.first(), keys.last()) else {
        return b.code.lookup_switch(default, keys);
    };
    let range = i64::from(max) - i64::from(min) + 1;
    if keys.len() as f64 / range as f64 >= MIN_SWITCH_DENSITY {
        let mut targets = vec![default; range as usize];
        for &(k, l) in keys.iter() {
            targets[(i64::from(k) - i64::from(min)) as usize] = l;
        }
        b.code.table_switch(min, default, &targets);
    } else {
        b.code.lookup_switch(default, keys);
    }
}

/// Java's `String.hashCode` of UTF-16 units `s`.
fn hash(s: &[u16]) -> i32 {
    s.iter().fold(0_i32, |h, &c| h.wrapping_mul(31).wrapping_add(i32::from(c)))
}
