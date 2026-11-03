// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The members of an intersection from those of its sides (dotty's `goAnd`, `Denotation.meet`
//! with `mergeSingleDenot`).

use super::Typer;
use crate::ctx::Ctx;
use tasty::tags::{PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified, TRAIT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The members of `site`, an intersection, from those of its sides `ma` and `mb` as selected
    /// in `ctx`: of each pair of matching term members the one dotty keeps, the others
    /// overloaded; none for two different type members or a matching pair dotty overloads
    /// (`needs_joint_member`).
    pub fn and_members(
        &mut self,
        site: TypeId,
        (ma, mb): (Vec<SymbolId>, Vec<SymbolId>),
        ctx: Option<Ctx>,
    ) -> Vec<SymbolId> {
        if ma.is_empty() || mb.is_empty() {
            return if ma.is_empty() { mb } else { ma };
        }
        let mut out = ma;
        for m in mb {
            if out.contains(&m) {
                continue;
            }
            if self.u.is_type_symbol(m) || out.iter().any(|&o| self.u.is_type_symbol(o)) {
                return Vec::new();
            }
            for &s in out.iter().chain([&m]) {
                self.complete(s);
            }
            match out.iter().position(|&o| {
                self.u.info_matches(site, o, m)
                    && self.same_member(site, o, m)
                    && self.same_erased_params(site, o, m)
            }) {
                Some(k) => match self.merged(site, (out[k], m), ctx) {
                    Some(s) => out[k] = s,
                    None => return Vec::new(),
                },
                None => out.push(m),
            }
        }
        out
    }

    /// Of matching members `a` and `b` of `site`, the one whose denotation dotty keeps: the one
    /// accessible in `ctx` if the other is not, else by `symScore` the one whose type conforms to
    /// the other's, else the one recorded as the joint member of both; `None` where dotty
    /// overloads both or joins members not both public.
    fn merged(&mut self, site: TypeId, (a, b): (SymbolId, SymbolId), ctx: Option<Ctx>) -> Option<SymbolId> {
        if let Some(ctx) = ctx {
            let (hidden_a, hidden_b) = (!self.is_accessible(a, ctx), !self.is_accessible(b, ctx));
            if hidden_a != hidden_b {
                return Some(if hidden_a { b } else { a });
            }
        }
        let score = self.sym_score(site, a, b);
        let (ia, ib) = (self.u.side_member_info(site, a), self.u.side_member_info(site, b));
        let (ra, rb) = (self.u.result_type(ia), self.u.result_type(ib));
        if score <= 0 && self.u.is_subtype(rb, ra) {
            return Some(b);
        }
        if score >= 0 && self.u.is_subtype(ra, rb) {
            return Some(a);
        }
        if self.visibility(a) < 2 || self.visibility(b) < 2 {
            return None;
        }
        self.u.info_meet(ia, ib)?;
        let s = if score >= 0 { a } else { b };
        self.u.set_joint_member(site, s, (a, b));
        Some(s)
    }

    /// Whether members `a` and `b` of `site` erase to the same parameter types, as their
    /// signatures match.
    fn same_erased_params(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
        let (ia, ib) = (self.u.own_member_info(site, a), self.u.own_member_info(site, b));
        self.u.erased_method_type(ia).0 == self.u.erased_method_type(ib).0
    }

    /// Whether both sides of intersection `t`, or of one nested in it, have members `name`, which
    /// `and_members` cannot merge into one; `t` a `this` stands for its given self type.
    pub fn needs_joint_member(&mut self, t: TypeId, name: &str) -> bool {
        let t = match *self.u.types().get(t) {
            Type::This(c) => {
                let Some(k) = self.u.class_symbol(c) else { return false };
                self.ensure_self(k);
                if self.u.given_self_type(k).is_none() {
                    return false;
                }
                self.u.self_type(k)
            }
            _ => {
                let w = self.u.widen(t);
                self.u.dealias(w)
            }
        };
        let Type::And(a, b) = *self.u.types().get(t) else { return false };
        let (ma, mb) = (self.type_members(a, name), self.type_members(b, name));
        !ma.is_empty() && !mb.is_empty()
            || ma.is_empty() && self.needs_joint_member(a, name)
            || mb.is_empty() && self.needs_joint_member(b, name)
    }

    /// Positive to pick `a`, negative to pick `b` (dotty's `symScore`): the concrete one, the one
    /// whose owner comes first in `site`'s base classes, the one more visible, the method.
    fn sym_score(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> i32 {
        let (ca, cb) = (self.is_concrete(a), self.is_concrete(b));
        if ca != cb {
            return if ca { 1 } else { -1 };
        }
        let (oa, ob) = (self.u.symbol(a).owner, self.u.symbol(b).owner);
        if let (Some(oa), Some(ob)) = (oa, ob)
            && oa != ob
        {
            if self.u.derives_from(oa, ob) {
                return 1;
            }
            if self.u.derives_from(ob, oa) {
                return -1;
            }
            if let Some(first) = self.base_classes(site).into_iter().find(|&c| c == oa || c == ob) {
                return if first == oa { 1 } else { -1 };
            }
        }
        let (va, vb) = (self.visibility(a), self.visibility(b));
        if va != vb {
            return if va > vb { 1 } else { -1 };
        }
        match (self.u.symbol(a).kind == Kind::Def, self.u.symbol(b).kind == Kind::Def) {
            (true, false) => 1,
            (false, true) => -1,
            _ => 0,
        }
    }

    /// How far `s` is visible, as its access boundary is wide: private, protected or qualified,
    /// public.
    fn visibility(&self, s: SymbolId) -> u8 {
        let flags = self.u.symbol(s).flags;
        if flags.has(PRIVATE) {
            return 0;
        }
        let qualified = flags.has(PRIVATEqualified) || self.syms.private_within.contains_key(&s);
        u8::from(!(qualified || flags.has(PROTECTED) || flags.has(PROTECTEDqualified))) + 1
    }

    /// The base classes of `t`, those of an intersection's right side not in its left side's
    /// first (dotty's `AndType.baseClasses`).
    fn base_classes(&mut self, t: TypeId) -> Vec<SymbolId> {
        let Type::And(l, r) = *self.u.types().get(t) else {
            return self.u.class_symbol(t).map(|c| self.u.linearization(c)).unwrap_or_default();
        };
        let (left, right) = (self.base_classes(l), self.base_classes(r));
        let mut out = Vec::new();
        for c in right {
            if !left.contains(&c) {
                out.push(c);
            } else if !self.u.symbol(c).flags.has(TRAIT) {
                break;
            }
        }
        out.extend(left);
        out
    }
}
