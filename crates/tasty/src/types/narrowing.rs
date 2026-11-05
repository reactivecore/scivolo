// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//! Term refinements that narrow the member they refine.

use super::{Type, TypeId};
use crate::{SymbolId, Universe};

impl Universe {
    /// The info a refinement of `prefix`'s widened type gives term member `m` of type `info`, if
    /// it narrows `info` with the same parameters (dotty's `infoMeet` of the refined member).
    pub fn narrowing_refinement(&mut self, prefix: TypeId, m: SymbolId, info: TypeId) -> Option<TypeId> {
        let name = self.symbol(m).name.clone();
        let refined = self.term_refinement_named(prefix, &name).filter(|&r| r != info)?;
        let (narrows, refined) = match (self.types().get(refined).clone(), self.types().get(info).clone()) {
            (Type::Method { .. }, Type::Method { .. }) => (self.method_narrows(refined, info), refined),
            (
                Type::Poly { binder, params: p, result: r },
                Type::Poly { binder: b2, params: q, result: s },
            ) if p.len() == q.len() => {
                let own = self.type_params(m);
                let s = self.map_type(s, &mut |u, t| {
                    let i = match *u.types().get(t) {
                        Type::Param(b, i) if b == b2 => i,
                        Type::Sym(None, x) => own.iter().position(|&o| o == x)? as u32,
                        _ => return None,
                    };
                    Some(u.intern_type(Type::Param(binder, i)))
                });
                (self.method_narrows(r, s), refined)
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
            (self.types().get(a).clone(), self.types().get(b).clone())
        else {
            return false;
        };
        let same = |u: &mut Self, x: TypeId, y: TypeId| x == y || u.is_subtype(x, y) && u.is_subtype(y, x);
        p.len() == q.len()
            && p.iter().zip(&q).all(|((_, x), (_, y))| same(self, *x, *y))
            && self.is_subtype(r, s)
    }
}
