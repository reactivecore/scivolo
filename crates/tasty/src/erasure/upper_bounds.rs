// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! A method type's parameter and result types with its type parameters replaced by their upper
//! bounds, before erasure.

use crate::types::{Type, TypeId};
use crate::{Binder, Universe};

impl Universe {
    /// `t` with its type parameters replaced by their upper bounds, given the bounds of the
    /// parameters of each polymorphic type; within `t`, only those bounded by a class or a type
    /// lambda, whose application is reduced until its constructor is no such parameter.
    pub fn upper_bound(&mut self, t: TypeId, bounds: &[(Binder, Vec<TypeId>)]) -> TypeId {
        self.upper_bound_except(t, bounds, &[])
    }

    /// `upper_bound` that does not reduce applications of the parameters in `reduced`.
    fn upper_bound_except(
        &mut self,
        t: TypeId,
        bounds: &[(Binder, Vec<TypeId>)],
        reduced: &[(Binder, u32)],
    ) -> TypeId {
        let bound =
            |b: Binder, i: u32| bounds.iter().find(|(binder, _)| *binder == b)?.1.get(i as usize).copied();
        let mut t = self.map_type(t, &mut |u, x| match u.types().get(x).clone() {
            Type::Param(b, i) => {
                let bounds = bound(b, i)?;
                let Type::Bounds(_, hi) = *u.types().get(bounds) else { return None };
                (!u.is_generic(bounds)).then_some(hi)
            }
            Type::Applied(..) => {
                let (mut applied, mut seen) = (x, reduced.to_vec());
                while let Type::Applied(tycon, args) = u.types().get(applied).clone()
                    && let Type::Param(b, i) = *u.types().get(tycon)
                    && !seen.contains(&(b, i))
                    && let Some(bounds) = bound(b, i)
                    && let Type::Bounds(_, hi) = *u.types().get(bounds)
                    && let Type::Lambda { binder, params, result } = u.types().get(hi).clone()
                {
                    seen.push((b, i));
                    let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
                    applied = u.subst(result, binder, &names, &args);
                }
                (applied != x).then(|| u.upper_bound_except(applied, bounds, &seen))
            }
            _ => None,
        });
        while let Type::Param(b, i) = *self.types().get(t) {
            let Some(bound) = bound(b, i) else { break };
            t = match *self.types().get(bound) {
                Type::Bounds(_, hi) => hi,
                _ => bound,
            };
        }
        t
    }
}
