//! Expected types of implicit searches with type variables outside the subset (unbounded ones,
//! ones in unions), or too unspecific to search; variables settled by their bounds.

use super::super::Typer;
use tasty::{Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Whether `t` has an uninstantiated type variable without bounds.
    pub fn has_unbounded_var(&mut self, t: TypeId) -> bool {
        let vars = self.open_vars(t);
        vars.into_iter().any(|v| self.constraint.bounds(v) == (tasty::NOTHING, tasty::ANY))
    }

    /// `t` with its type variables whose bounds are equal instantiated to them.
    pub fn determined(&mut self, t: TypeId) -> TypeId {
        let t = self.zonk(t);
        for v in self.open_vars(t) {
            let (lo, hi) = self.constraint.bounds(v);
            if lo == hi {
                self.constraint.instantiate(v, lo);
            }
        }
        self.zonk(t)
    }

    /// Whether `t` is `Any`, `AnyRef`, or an uninstantiated type variable bounded by them, which no
    /// search may answer (dotty's `isUnderspecified`).
    pub fn is_underspecified(&mut self, t: TypeId) -> bool {
        let t = self.zonk(t);
        match self.u.types().get(t).clone() {
            Type::Var(v) => {
                let (_, hi) = self.constraint.bounds(v);
                self.is_underspecified(hi)
            }
            Type::Or(a, b, _) if self.u.class_symbol(b) == Some(self.u.builtins().null) => {
                self.is_underspecified(a)
            }
            Type::Sym(..) | Type::TypeRef(..) => {
                let b = self.u.builtins();
                let t = self.u.dealias(t);
                let class = match self.u.types().get(t).clone() {
                    Type::Sym(_, s) => Some(s),
                    Type::TypeRef(p, n) => self.u.resolve(p, &n, true),
                    _ => None,
                };
                class.is_some_and(|c| c == b.any || c == b.object)
            }
            _ => false,
        }
    }

    /// Whether `t`, its aliases expanded, has a union with an uninstantiated type variable in it.
    pub fn has_open_union(&mut self, t: TypeId) -> bool {
        self.open_union_within(t, 16)
    }

    fn open_union_within(&mut self, t: TypeId, depth: u32) -> bool {
        let Some(depth) = depth.checked_sub(1) else { return false };
        let t = self.u.dealias(t);
        match self.u.types().get(t).clone() {
            Type::Or(..) => !self.open_vars(t).is_empty(),
            Type::And(a, b) => self.open_union_within(a, depth) || self.open_union_within(b, depth),
            Type::Applied(_, args) => args.into_iter().any(|a| self.open_union_within(a, depth)),
            Type::Annotated(a, _) | Type::ByName(a) => self.open_union_within(a, depth),
            Type::Bounds(lo, hi) => self.open_union_within(lo, depth) || self.open_union_within(hi, depth),
            _ => false,
        }
    }
}
