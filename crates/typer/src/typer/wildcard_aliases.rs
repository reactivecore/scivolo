// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeApplications.scala
//! Type aliases applied to wildcards (dotty's `appliedTo` with its `Reducer`): reducible when
//! each parameter of the alias occurs only as an argument of an applied type.

use super::Typer;
use tasty::{SymbolId, Type, TypeId, Universe};

impl Typer<'_> {
    /// Whether type alias `s` of a type lambda can be applied to wildcard arguments: its
    /// parameters occur in its right-hand side only as arguments of applied types.
    pub fn reduces_with_wildcards(&mut self, s: SymbolId) -> bool {
        self.complete(s);
        let info = self.u.info(s);
        let lambda = self.u.type_alias(info).unwrap_or(info);
        let Some(lambda) = self.u.lambda(lambda) else { return false };
        let (binder, result, names) = (lambda.binder, lambda.result, lambda.names());
        let is_param = |u: &Universe, t: TypeId| match u.ty(t) {
            Type::Param(b, _) => *b == binder,
            Type::Local(b, n) => b.0 == binder.0 && names.contains(n),
            _ => false,
        };
        let mut total = 0;
        self.u.map_type(result, &mut |u, part| {
            total += usize::from(is_param(u, part));
            None
        });
        as_args(self.u, result, &is_param) == Some(total)
    }

    /// Alias `t` applied to wildcards and reduced, with its refinements of a wildcard bounded by it.
    pub fn bounded_refinements(&mut self, t: TypeId) -> TypeId {
        let t = self.u.dealias(t);
        bounded(self.u, t)
    }
}

fn bounded(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, part| {
        let Type::Refined(parent, name, info) = *u.ty(part) else { return None };
        let b = u.type_alias(info)?;
        u.bounds(b)?;
        let parent = bounded(u, parent);
        Some(u.intern_type(Type::Refined(parent, name, b)))
    })
}

/// How many of the parts `is_param` accepts occur in `t` as arguments of applied types, through
/// applications, intersections, unions and annotations; `None` if one occurs elsewhere first.
fn as_args(u: &Universe, t: TypeId, is_param: &dyn Fn(&Universe, TypeId) -> bool) -> Option<usize> {
    if is_param(u, t) {
        return None;
    }
    match *u.ty(t) {
        Type::Applied(tycon, ref args) => {
            let mut n = as_args(u, tycon, is_param)?;
            for &a in args {
                n += if is_param(u, a) { 1 } else { as_args(u, a, is_param)? };
            }
            Some(n)
        }
        Type::And(a, b) | Type::Or(a, b, _) => Some(as_args(u, a, is_param)? + as_args(u, b, is_param)?),
        Type::Annotated(a, _) => as_args(u, a, is_param),
        Type::Refined(parent, _, info) => {
            let n = as_args(u, parent, is_param)?;
            Some(n + usize::from(u.type_alias(info).is_some_and(|a| is_param(u, a))))
        }
        _ => Some(0),
    }
}
