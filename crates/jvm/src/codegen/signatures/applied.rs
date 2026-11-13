// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Applied higher-kinded method type parameters (dotty's `resolveAppliedType`), and erasure with
//! the method type parameters in scope bounded.

use super::{Boxing, Sig};
use tasty::{Binder, Erased, TypeId};

/// What an applied method type parameter stands for in a signature.
pub enum Applied {
    /// Its bound, a type lambda, applied to the arguments.
    Resolved(TypeId),
    /// The parameter itself, its bound being no type lambda.
    Param(String),
    /// Its erasure, the applied bound referring to the parameter again.
    Erased,
}

impl Sig<'_> {
    /// `tycon[args]`, if `tycon` is a method type parameter in scope.
    pub fn applied_param(&mut self, tycon: TypeId, args: &[TypeId]) -> Option<Applied> {
        let (b, i) = self.u.param_ref(tycon)?;
        let (name, bounds) = self.poly_param(b, i)?;
        let hi = self.u.bounds(bounds).map_or(bounds, |(_, hi)| hi);
        let Some(lambda) = self.u.lambda(hi) else { return Some(Applied::Param(name)) };
        let (binder, names, result) = (lambda.binder, lambda.names(), lambda.result);
        let t = self.u.subst(result, binder, &names, args);
        let mut recursive = false;
        self.u.map_type(t, &mut |_, x| {
            recursive |= x == tycon;
            None
        });
        Some(if recursive { Applied::Erased } else { Applied::Resolved(t) })
    }

    /// Appends the erasure of `t`, the method type parameters in scope replaced by their bounds.
    pub fn erased(&mut self, t: TypeId, boxing: Boxing) {
        let t = self.bounded(t);
        let e = self.u.erase(t);
        self.erased_sig(&e, boxing);
    }

    /// Determines `t` with the method type parameters in scope replaced by their upper bounds.
    pub fn bounded(&mut self, t: TypeId) -> TypeId {
        let bounds: Vec<(Binder, Vec<TypeId>)> =
            self.poly.iter().map(|(b, ps)| (*b, ps.iter().map(|p| p.1).collect())).collect();
        self.u.upper_bound(t, &bounds)
    }

    /// Whether a method type parameter of `bounds` erases to a primitive, also through a value
    /// class, and so appears by its erasure.
    pub fn primitive_bound(&mut self, bounds: TypeId) -> bool {
        let hi = self.u.bounds(bounds).map_or(bounds, |(_, hi)| hi);
        match self.u.erase_value(hi) {
            Erased::Value(_, e) => matches!(*e, Erased::Prim(_)),
            e => matches!(e, Erased::Prim(_)),
        }
    }
}
