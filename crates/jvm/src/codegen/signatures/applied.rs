// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Applied higher-kinded method type parameters (dotty's `resolveAppliedType`), and erasure with
//! the method type parameters in scope bounded.

use super::{Boxing, Sig};
use tasty::{Binder, Type, TypeId};

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
        let Type::Param(b, i) = *self.u.types().get(tycon) else { return None };
        let (name, bounds) = self.poly_param(b, i)?;
        let hi = match *self.u.types().get(bounds) {
            Type::Bounds(_, hi) => hi,
            _ => bounds,
        };
        let Type::Lambda { binder, params, result } = self.u.types().get(hi).clone() else {
            return Some(Applied::Param(name));
        };
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
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
        let bounds: Vec<(Binder, Vec<TypeId>)> =
            self.poly.iter().map(|(b, ps)| (*b, ps.iter().map(|p| p.1).collect())).collect();
        let t = self.u.upper_bound(t, &bounds);
        let e = self.u.erase(t);
        self.erased_sig(&e, boxing);
    }
}
