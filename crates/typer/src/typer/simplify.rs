// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//! Simplified types (dotty's `TypeOps.simplify`).

use super::Typer;
use super::seen_from::member_of;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `t` without `@uncheckedVariance` annotations, its unions and intersections the lubs and
    /// glbs of their parts (dotty's `TypeOps.simplify` of a term's type).
    pub fn simplified(&mut self, t: TypeId) -> TypeId {
        simplify(self.u, t)
    }

    /// `simplified`, match types reduced and compile-time operations folded too.
    pub fn simplified_inferred(&mut self, t: TypeId) -> TypeId {
        let t = normalize_all(self.u, t);
        simplify(self.u, t)
    }
}

fn simplify(u: &mut tasty::Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, part| match *u.types().get(part) {
        Type::Annotated(inner, annot) => {
            let class = u.class_symbol(annot);
            class
                .is_some_and(|c| member_of(u, c, "scala.annotation.unchecked", |n| n == "uncheckedVariance"))
                .then(|| simplify(u, inner))
        }
        Type::Or(a, b, soft) => {
            let (a, b) = (simplify(u, a), simplify(u, b));
            Some(u.lub_as(a, b, soft))
        }
        Type::And(a, b) => {
            let (a, b) = (simplify(u, a), simplify(u, b));
            Some(u.glb(a, b))
        }
        Type::Super(..) | Type::Skolem(..) => Some(part),
        _ => None,
    })
}

/// `t` with match types reduced and compile-time operations folded, everywhere (dotty's
/// `simplified` of an inferred type).
pub fn normalize_all(u: &mut tasty::Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, part| match *u.types().get(part) {
        Type::Applied(tycon, _) if matches!(u.types().get(tycon), Type::Lambda { .. }) => {
            let reduced = u.beta_reduced(part);
            Some(normalize_all(u, reduced))
        }
        Type::Applied(..) | Type::Match { .. } => u.try_normalize(part).map(|n| normalize_all(u, n)),
        _ => None,
    })
}
