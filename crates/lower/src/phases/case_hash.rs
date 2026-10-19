// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//! `hashCode` of case classes as dotty's `SyntheticMembers` writes it (`chooseHashcode`,
//! `caseHashCodeBody`, `hashImpl`).

use crate::Lower;
use crate::definitions::{CaseClasses, symbol};
use syntax::ast::Span;
use tasty::tags::{MUTABLE, SYNTHETIC};
use tasty::{Erased, Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

impl Lower<'_> {
    /// The hash code of a case class: a constant without fields, mixed fields if any is
    /// primitive, else `MurmurHash3.productHash` (dotty's `chooseHashcode`); its local, owned by
    /// `class` until its method exists, goes to `locals`.
    pub fn hash_code_body(
        &mut self,
        class: SymbolId,
        accessors: &[SymbolId],
        own_name: &str,
        locals: &mut Vec<SymbolId>,
        cc: CaseClasses,
    ) -> TNodeId {
        let name_hash = string_hash(own_name);
        if accessors.is_empty() {
            return self.int_literal(name_hash);
        }
        let primitive = accessors.iter().any(|&a| {
            let info = self.u.info(a);
            matches!(self.u.erase(info), Erased::Prim(_) | Erased::Unit)
        });
        if !primitive {
            let module = self.ident(cc.murmur_hash3, Span::NONE);
            let this = self.this(class, Span::NONE);
            let seed = self.int_literal(mix(0xcafebabe_u32 as i32, name_hash));
            let ignore_prefix = self.boolean_literal(true);
            return self.call(module, cc.product_hash, &[this, seed, ignore_prefix]);
        }
        let flags = Flags::default().with(MUTABLE).with(SYNTHETIC);
        let acc = self.u.define(symbol("acc", Kind::Val, class, flags));
        locals.push(acc);
        let int = self.u.class_ref("scala.Int");
        self.u.set_info(acc, int);
        let init = self.int_literal(0xcafebabe_u32 as i32);
        let name = self.term_name("acc");
        let tpt = self.type_tree(int, Span::NONE);
        let def = Tree::ValDef { name, tpt, rhs: Some(init) };
        let mut stats = vec![self.tpd.node(def, Span::NONE, Some(int), Some(acc))];
        let prefix = self.int_literal(name_hash);
        stats.push(self.mix_into(acc, prefix, cc.mix));
        for &a in accessors {
            let hash = self.field_hash(a, cc);
            stats.push(self.mix_into(acc, hash, cc.mix));
        }
        let acc_ref = self.ident(acc, Span::NONE);
        let size = self.int_literal(accessors.len() as i32);
        let finish = self.static_call(cc.finalize_hash, &[acc_ref, size]);
        let block = self.block(&stats, finish, Span::NONE);
        self.positioned(block)
    }

    /// `acc = mix(acc, hash)` for `mix` the method `Statics.mix`.
    fn mix_into(&mut self, acc: SymbolId, hash: TNodeId, mix: SymbolId) -> TNodeId {
        let lhs = self.ident(acc, Span::NONE);
        let current = self.ident(acc, Span::NONE);
        let rhs = self.static_call(mix, &[current, hash]);
        let unit = self.u.class_ref("scala.Unit");
        let assign = self.tpd.node(Tree::Assign { lhs, rhs }, Span::NONE, Some(unit), None);
        self.positioned(assign)
    }

    /// The hash of field `a` (dotty's `hashImpl`).
    fn field_hash(&mut self, a: SymbolId, cc: CaseClasses) -> TNodeId {
        let value = self.ident(a, Span::NONE);
        let info = self.u.info(a);
        match self.u.erase(info) {
            Erased::Unit | Erased::Null => self.int_literal(0),
            Erased::Prim('z') => {
                let (yes, no) = (self.int_literal(1231), self.int_literal(1237));
                let int = self.u.class_ref("scala.Int");
                let t = Tree::If { cond: value, thenp: yes, elsep: no, inline: false };
                let t = self.tpd.node(t, Span::NONE, Some(int), None);
                self.positioned(t)
            }
            Erased::Prim('i') => value,
            Erased::Prim('s' | 'b' | 'c') => {
                let class = self.class_of(info).expect("a primitive class");
                let to_int = self.u.decl(class, "toInt").unwrap_or_default().into_iter().next();
                let sel = self.select(value, to_int.expect("`toInt`"), Span::NONE);
                let int = self.u.class_ref("scala.Int");
                self.tpd.get_mut(sel).tpe = Some(int);
                self.positioned(sel)
            }
            Erased::Prim('j') => self.static_call(cc.long_hash, &[value]),
            Erased::Prim('d') => self.static_call(cc.double_hash, &[value]),
            Erased::Prim('f') => self.static_call(cc.float_hash, &[value]),
            _ => self.static_call(cc.any_hash, &[value]),
        }
    }
}

/// Java's `String.hashCode` of `s`.
fn string_hash(s: &str) -> i32 {
    s.encode_utf16().fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)))
}

/// Scala's `Statics.mix`.
fn mix(hash: i32, data: i32) -> i32 {
    let k = data.wrapping_mul(0xcc9e2d51_u32 as i32).rotate_left(15).wrapping_mul(0x1b873593);
    let h = (hash ^ k).rotate_left(13);
    h.wrapping_mul(5).wrapping_add(0xe6546b64_u32 as i32)
}
