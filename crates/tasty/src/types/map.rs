//! Rebuilding a type with parts replaced.

use super::{Type, TypeId};
use crate::Universe;

impl Universe {
    /// `t` with every part for which `f` gives a replacement replaced, outermost parts first;
    /// the other parts are rebuilt from their mapped children.
    pub fn map_type(&mut self, t: TypeId, f: &mut dyn FnMut(&mut Self, TypeId) -> Option<TypeId>) -> TypeId {
        if let Some(r) = f(self, t) {
            return r;
        }
        let mut m = |u: &mut Self, x: TypeId| u.map_type(x, f);
        let params =
            |u: &mut Self, ps: Vec<(String, TypeId)>, m: &mut dyn FnMut(&mut Self, TypeId) -> TypeId| {
                ps.into_iter().map(|(n, p)| (n, m(u, p))).collect::<Vec<_>>()
            };
        let mapped = match self.types().get(t).clone() {
            Type::TypeRef(p, n) => Type::TypeRef(m(self, p), n),
            Type::TermRef(p, n) => Type::TermRef(m(self, p), n),
            Type::Sym(Some(p), s) => Type::Sym(Some(m(self, p)), s),
            Type::This(c) => Type::This(m(self, c)),
            Type::Applied(c, args) => {
                let c = m(self, c);
                Type::Applied(c, args.into_iter().map(|a| m(self, a)).collect())
            }
            Type::Alias(a) => Type::Alias(m(self, a)),
            Type::Bounds(lo, hi) => Type::Bounds(m(self, lo), m(self, hi)),
            Type::And(a, b) => Type::And(m(self, a), m(self, b)),
            Type::Or(a, b) => Type::Or(m(self, a), m(self, b)),
            Type::Annotated(a, annot) => Type::Annotated(m(self, a), annot),
            Type::ByName(a) => Type::ByName(m(self, a)),
            Type::Refined(p, n, i) => Type::Refined(m(self, p), n, m(self, i)),
            Type::Rec(b, a) => Type::Rec(b, m(self, a)),
            Type::Match(a) => Type::Match(m(self, a)),
            Type::Wildcard(Some(b)) => Type::Wildcard(Some(m(self, b))),
            Type::Method { binder, params: ps, result, implicit } => {
                let params = params(self, ps, &mut m);
                Type::Method { binder, params, result: m(self, result), implicit }
            }
            Type::Poly { binder, params: ps, result } => {
                let params = params(self, ps, &mut m);
                Type::Poly { binder, params, result: m(self, result) }
            }
            Type::Lambda { binder, params: ps, result } => {
                let params = params(self, ps, &mut m);
                Type::Lambda { binder, params, result: m(self, result) }
            }
            _ => return t,
        };
        self.intern_type(mapped)
    }
}
