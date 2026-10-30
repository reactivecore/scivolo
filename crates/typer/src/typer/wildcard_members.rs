//! Result types that are a wildcard's bounds.

use tasty::{Type, TypeId, Universe};

/// Whether `t` is a wildcard's bounds, or an intersection or union with one.
pub fn is_bounds_like(u: &Universe, t: TypeId) -> bool {
    match *u.types().get(t) {
        Type::Bounds(..) => true,
        Type::And(a, b) | Type::Or(a, b, _) => is_bounds_like(u, a) || is_bounds_like(u, b),
        _ => false,
    }
}
