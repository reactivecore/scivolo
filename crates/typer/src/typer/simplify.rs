// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//! Simplified types (dotty's `TypeOps.simplify`).

use super::Typer;
use super::seen_from::member_of;
use super::type_ops::TypeOps;
use tasty::{Type, TypeId};

impl TypeOps<'_> {
    /// Simplifies `t`: drops `@uncheckedVariance` annotations, makes its unions and intersections
    /// the lubs and glbs of their parts, dealiases type projections on classes (dotty's
    /// `TypeOps.simplify` of a term's type).
    pub fn simplified(&mut self, t: TypeId) -> TypeId {
        simplify(self.u, t, false)
    }
}

impl Typer<'_> {
    /// `simplified`, match types reduced and compile-time operations folded too; a reduction
    /// that reaches its own type again is outside the subset.
    pub fn simplified_inferred(&mut self, t: TypeId) -> TypeId {
        self.simplified_inferred_as(t, false)
    }

    /// `simplified_inferred`, keeping `@uncheckedVariance` annotations if `keep_unchecked`
    /// (dotty's `SimplifyKeepUnchecked`).
    pub fn simplified_inferred_as(&mut self, t: TypeId, keep_unchecked: bool) -> TypeId {
        let mut cyclic = false;
        let t = normalize_outside(self.u, t, &mut Vec::new(), &mut cyclic);
        if cyclic {
            self.unsupported_at(syntax::ast::Span::NONE, "recursive match type reduction");
        }
        simplify(self.u, t, keep_unchecked)
    }
}

fn simplify(u: &mut tasty::Universe, t: TypeId, keep: bool) -> TypeId {
    u.map_type(t, &mut |u, part| {
        let ty = *u.ty(part);
        match ty {
            Type::Annotated(inner, annot) if !keep => {
                let class = u.class_symbol(annot);
                class
                    .is_some_and(|c| {
                        member_of(u, c, "scala.annotation.unchecked", |n| n == "uncheckedVariance")
                    })
                    .then(|| simplify(u, inner, keep))
            }
            Type::Or(a, b, soft) => {
                let (a, b) = (simplify(u, a, keep), simplify(u, b, keep));
                Some(u.lub_as(a, b, soft))
            }
            Type::And(a, b) => {
                let (a, b) = (simplify(u, a, keep), simplify(u, b, keep));
                Some(u.glb(a, b))
            }
            Type::Super(..) | Type::Skolem(..) => Some(part),
            Type::Sym(Some(p), s) if u.symbol(s).kind == tasty::Kind::Type && is_class_type(u, p) => {
                let alias = u.referenced_alias(part)?;
                let lambda = u.lambda(alias).is_some();
                (!lambda).then(|| {
                    let dealiased = u.dealias(alias);
                    simplify(u, dealiased, keep)
                })
            }
            _ => None,
        }
    })
}

/// Whether `t` is a class type and not a path: a projection `t#X` of an alias `X` is its alias
/// (dotty's `reduceProjection`).
fn is_class_type(u: &mut tasty::Universe, t: TypeId) -> bool {
    match *u.ty(t) {
        Type::Sym(_, c) => u.symbol(c).kind == tasty::Kind::Class,
        Type::Applied(tycon, _) => {
            u.class_symbol(tycon).is_some_and(|c| u.symbol(c).kind == tasty::Kind::Class)
        }
        _ => false,
    }
}

/// `t` with match types reduced and compile-time operations folded, everywhere (dotty's
/// `simplified` of an inferred type); a type reached again inside its own reduction is left as
/// it is.
pub fn normalize_all(u: &mut tasty::Universe, t: TypeId) -> TypeId {
    normalize_outside(u, t, &mut Vec::new(), &mut false)
}

fn normalize_outside(
    u: &mut tasty::Universe,
    t: TypeId,
    reducing: &mut Vec<TypeId>,
    cyclic: &mut bool,
) -> TypeId {
    u.map_type(t, &mut |u, part| {
        let reduced = match *u.ty(part) {
            _ if reducing.contains(&part) => {
                *cyclic = true;
                return Some(part);
            }
            Type::Applied(tycon, _) if u.lambda(tycon).is_some() => u.beta_reduced(part),
            Type::Applied(..) | Type::Match { .. } => u.try_normalize(part)?,
            _ => return None,
        };
        reducing.push(part);
        let normal = normalize_outside(u, reduced, reducing, cyclic);
        reducing.pop();
        Some(normal)
    })
}
