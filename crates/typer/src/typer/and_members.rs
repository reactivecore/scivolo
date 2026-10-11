//! The members of an intersection from those of its sides (dotty's `goAnd`, `Denotation.meet`
//! with `mergeSingleDenot`).

use super::Typer;
use crate::ctx::Ctx;
use tasty::tags::{PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified, TRAIT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The members of `site`, an intersection, from those of its sides `ma` and `mb` as selected
    /// in `ctx`: of each pair of matching term members the one dotty keeps, the others
    /// overloaded; none for two different type members or a pair dotty joins
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
            match out.iter().position(|&o| self.same_member(site, o, m)) {
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
    /// the other's; `None` where dotty joins both with the intersection of their types.
    fn merged(&mut self, site: TypeId, (a, b): (SymbolId, SymbolId), ctx: Option<Ctx>) -> Option<SymbolId> {
        if let Some(ctx) = ctx {
            let (hidden_a, hidden_b) = (!self.is_accessible(a, ctx), !self.is_accessible(b, ctx));
            if hidden_a != hidden_b {
                return Some(if hidden_a { b } else { a });
            }
        }
        let score = self.sym_score(site, a, b);
        let (ia, ib) = (self.u.member_info(site, a), self.u.member_info(site, b));
        let (ra, rb) = (self.u.result_type(ia), self.u.result_type(ib));
        if score <= 0 && self.u.is_subtype(rb, ra) {
            return Some(b);
        }
        (score >= 0 && self.u.is_subtype(ra, rb)).then_some(a)
    }

    /// Whether both sides of intersection `t` have members `name`, which `and_members` cannot
    /// merge into one.
    pub fn needs_joint_member(&mut self, t: TypeId, name: &str) -> bool {
        let t = self.u.widen(t);
        let t = self.u.dealias(t);
        let Type::And(a, b) = *self.u.types().get(t) else { return false };
        !self.type_members(a, name).is_empty() && !self.type_members(b, name).is_empty()
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
