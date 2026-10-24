// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Members of a value whose type gives a class type parameter a wildcard argument: a parameter
//! or result of that type parameter's type typed as the parameter selected on the value
//! (dotty's `argForParam`).

use super::Typer;
use tasty::{Kind, Type, TypeId, Universe};

impl Typer<'_> {
    /// Method type `seen`, member info `raw` seen from `prefix`, with each parameter and final
    /// result whose type is a class type parameter that the type of `prefix`, a stable value path,
    /// gives a wildcard argument typed as that parameter of `prefix`.
    pub fn wildcard_params(&mut self, raw: TypeId, seen: TypeId, prefix: TypeId) -> TypeId {
        let path = match *self.u.types().get(prefix) {
            Type::Sym(_, v) => {
                matches!(self.u.symbol(v).kind, Kind::Val | Kind::Param)
                    && super::stability::is_stable_path(self.u, prefix)
            }
            _ => false,
        };
        let (Type::Method { params: raw_params, result: raw_result, .. }, true) =
            (self.u.types().get(raw).clone(), path)
        else {
            return seen;
        };
        let Type::Method { binder, mut params, result, implicit, contextual } =
            self.u.types().get(seen).clone()
        else {
            return seen;
        };
        for ((_, p), &(_, r)) in params.iter_mut().zip(&raw_params) {
            *p = self.wildcard_param(r, prefix).unwrap_or(*p);
        }
        let result = match self.wildcard_param(raw_result, prefix) {
            Some(r) => r,
            None => self.wildcard_params(raw_result, result, prefix),
        };
        self.u.intern_type(Type::Method { binder, params, result, implicit, contextual })
    }

    /// `prefix.T` if `raw` is class type parameter `T` and `prefix`'s type gives it a wildcard.
    fn wildcard_param(&mut self, raw: TypeId, prefix: TypeId) -> Option<TypeId> {
        let (class, i) = self.u.class_param(raw)?;
        let widened = self.u.widen(prefix);
        let base = self.u.base_type(widened, class)?;
        let Type::Applied(_, args) = self.u.types().get(base).clone() else { return None };
        matches!(self.u.types().get(*args.get(i)?), Type::Bounds(..)).then_some(())?;
        let tparam = self.u.type_params(class)[i];
        Some(self.u.intern_type(Type::Sym(Some(prefix), tparam)))
    }
}

/// Whether `t` is a wildcard's bounds, or an intersection or union with one.
pub fn is_bounds_like(u: &Universe, t: TypeId) -> bool {
    match *u.types().get(t) {
        Type::Bounds(..) => true,
        Type::And(a, b) | Type::Or(a, b, _) => is_bounds_like(u, a) || is_bounds_like(u, b),
        _ => false,
    }
}
