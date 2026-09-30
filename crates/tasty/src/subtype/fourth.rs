//! The last try of a comparison (dotty's `fourthTry`), by the left-hand side: abstract types
//! through their upper bound, `Nothing` and `Null`, singletons, intersections.

use super::Cmp;
use super::refs::Meaning;
use crate::types::{Type, TypeId};

impl Cmp<'_> {
    pub fn fourth_try(&mut self, a: TypeId, b: TypeId) -> bool {
        match self.u.types().get(a).clone() {
            Type::TypeRef(..) | Type::TermRef(..) | Type::Sym(..) => match self.named(a).map(|n| n.meaning) {
                Some(Meaning::Abstract(_, hi)) => self.recur(hi, b),
                Some(Meaning::Class(c)) if c == self.u.builtins().nothing => true,
                Some(Meaning::Class(c)) if c == self.u.builtins().null => self.nullable(b),
                Some(Meaning::Term(_)) => self.below_singleton(a, b),
                _ => false,
            },
            Type::Applied(tycon, args) => self.compare_applied1(a, b, tycon, &args),
            Type::This(_) | Type::Constant(..) | Type::RecThis(_) => self.below_singleton(a, b),
            Type::Refined(parent, ..)
            | Type::Rec(_, parent)
            | Type::Annotated(parent)
            | Type::Match(parent) => self.recur(parent, b),
            Type::And(x, y) => self.and_below(a, b, x, y),
            _ => false,
        }
    }

    /// `x & y <: b`: through `b` dealiased, `|` in the intersection distributed, else one of
    /// the parts.
    fn and_below(&mut self, a: TypeId, b: TypeId, x: TypeId, y: TypeId) -> bool {
        let dealiased = self.u.dealias(b);
        if dealiased != b {
            return self.recur(a, dealiased);
        }
        for (or, other, left) in [(x, y, true), (y, x, false)] {
            if let Type::Or(p, q) = self.u.types().get(or).clone() {
                let inter = |s: &mut Self, t| match left {
                    true => s.u.intern_type(Type::And(t, other)),
                    false => s.u.intern_type(Type::And(other, t)),
                };
                let (ap, aq) = (inter(self, p), inter(self, q));
                return self.recur(ap, b) && self.recur(aq, b);
            }
        }
        self.either(|s| s.recur(x, b), |s| s.recur(y, b))
    }

    /// `a <: b` for a singleton `a`: the same path, or `a` widened.
    fn below_singleton(&mut self, a: TypeId, b: TypeId) -> bool {
        let path = matches!(self.u.types().get(b), Type::TermRef(..) | Type::This(_) | Type::Sym(..));
        if path && self.same_path(a, b) {
            return true;
        }
        let widened = self.u.widen(a);
        widened != a && self.recur(widened, b)
    }

    /// Whether paths `a` and `b` denote the same value: the same term, or an object and `this`
    /// of its class.
    fn same_path(&mut self, a: TypeId, b: TypeId) -> bool {
        let (Some(s1), Some(s2)) = (self.path_symbol(a), self.path_symbol(b)) else { return false };
        let module_class = |s: &mut Self, m| match s.u.symbol(m).kind {
            crate::Kind::Val => s.u.module_class(m),
            _ => Some(m),
        };
        s1 == s2 || module_class(self, s1).is_some() && module_class(self, s1) == module_class(self, s2)
    }
}
