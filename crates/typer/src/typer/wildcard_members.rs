// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Members of a value that is no path and whose type gives a class type parameter a wildcard
//! argument: a parameter of that type parameter's type typed as the wildcard's lower bound, a
//! result as its upper bound (dotty's `argForParam` on the widened prefix, compared by bounds).

use super::Typer;
use tasty::{Type, TypeId, Universe};

impl Typer<'_> {
    /// Method type `seen`, a member's info seen from `prefix`, with each parameter that is a
    /// wildcard's bounds replaced by the lower bound and the final result by the upper bound,
    /// unless `prefix` is a stable path, whose wildcards `as_seen_from` selects on it.
    pub fn wildcard_params(&mut self, seen: TypeId, prefix: TypeId) -> TypeId {
        if super::stability::is_stable_path(self.u, prefix) {
            return seen;
        }
        let Type::Method { binder, mut params, result, implicit, contextual } =
            self.u.types().get(seen).clone()
        else {
            return seen;
        };
        for (_, p) in &mut params {
            if let Type::Bounds(lo, _) = *self.u.types().get(*p) {
                *p = lo;
            }
        }
        let result = match *self.u.types().get(result) {
            Type::Bounds(_, hi) => hi,
            _ => self.wildcard_params(result, prefix),
        };
        self.u.intern_type(Type::Method { binder, params, result, implicit, contextual })
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
