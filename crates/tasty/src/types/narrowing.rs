// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//! Term refinements that narrow the member they refine.

use super::{Type, TypeId};
use crate::{SymbolId, Universe};

impl Universe {
    /// The info a refinement of `prefix`'s widened type gives term member `m` of type `info`, if
    /// it narrows `info` with the same parameters (dotty's `infoMeet` of the refined member).
    pub fn narrowing_refinement(&mut self, prefix: TypeId, m: SymbolId, info: TypeId) -> Option<TypeId> {
        let name = self.interned_name(m);
        let refined = self.term_refinement_named(prefix, name).filter(|&r| r != info)?;
        let (narrows, refined) = match (*self.ty(refined), *self.ty(info)) {
            (Type::Method { .. }, Type::Method { .. }) => (self.method_narrows(refined, info), refined),
            (
                Type::Poly { binder, params: p, result: r },
                Type::Poly { binder: b2, params: q, result: s },
            ) if self.params_of(p).len() == self.params_of(q).len() => {
                let (p, q) = (self.params_of(p).to_vec(), self.params_of(q).to_vec());
                let own = self.type_params(m);
                let ours = |u: &mut Self, t| {
                    u.map_type(t, &mut |u, t| {
                        let i = match *u.ty(t) {
                            Type::Param(b, i) if b == b2 => i,
                            Type::Sym(None, x) => own.iter().position(|&o| o == x)? as u32,
                            _ => return None,
                        };
                        Some(u.intern_type(Type::Param(binder, i)))
                    })
                };
                let s = ours(self, s);
                let bounds: Vec<TypeId> = q.iter().map(|&(_, b)| ours(self, b)).collect();
                let same_bounds = p.iter().zip(bounds).all(|(&(_, a), b)| self.same_bounds(a, b));
                (same_bounds && self.method_narrows(r, s), refined)
            }
            (Type::ByName(r), Type::ByName(s)) => (self.is_subtype(r, s), refined),
            (Type::Method { .. } | Type::Poly { .. }, _) => return None,
            (_, Type::Method { .. } | Type::Poly { .. } | Type::ByName(_)) => return None,
            (Type::ByName(r), _) => (self.is_subtype(r, info), r),
            _ => (self.is_subtype(refined, info), refined),
        };
        narrows.then_some(refined)
    }

    /// Whether method type `a` has the parameter types of `b` and a result conforming to its.
    fn method_narrows(&mut self, a: TypeId, b: TypeId) -> bool {
        let (Type::Method { params: p, result: r, .. }, Type::Method { params: q, result: s, .. }) =
            (*self.ty(a), *self.ty(b))
        else {
            return false;
        };
        let (p, q) = (self.params_of(p).to_vec(), self.params_of(q).to_vec());
        p.len() == q.len()
            && p.iter().zip(&q).all(|((_, x), (_, y))| self.is_same(*x, *y))
            && self.is_subtype(r, s)
    }

    /// Whether type parameter bounds `a` and `b` are the same.
    fn same_bounds(&mut self, a: TypeId, b: TypeId) -> bool {
        match (*self.ty(a), *self.ty(b)) {
            (Type::Bounds(l1, h1), Type::Bounds(l2, h2)) => self.is_same(l1, l2) && self.is_same(h1, h2),
            _ => self.is_same(a, b),
        }
    }

    fn is_same(&mut self, x: TypeId, y: TypeId) -> bool {
        x == y || self.is_subtype(x, y) && self.is_subtype(y, x)
    }
}
