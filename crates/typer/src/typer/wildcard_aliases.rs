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
        let lambda = match *self.u.types().get(info) {
            Type::Alias(a) => a,
            _ => info,
        };
        let Type::Lambda { binder, ref params, result } = *self.u.types().get(lambda) else { return false };
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let is_param = |u: &Universe, t: TypeId| match u.types().get(t) {
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
}

/// How many of the parts `is_param` accepts occur in `t` as arguments of applied types, through
/// applications, intersections, unions and annotations; `None` if one occurs elsewhere first.
fn as_args(u: &Universe, t: TypeId, is_param: &dyn Fn(&Universe, TypeId) -> bool) -> Option<usize> {
    if is_param(u, t) {
        return None;
    }
    match u.types().get(t).clone() {
        Type::Applied(tycon, args) => {
            let mut n = as_args(u, tycon, is_param)?;
            for a in args {
                n += if is_param(u, a) { 1 } else { as_args(u, a, is_param)? };
            }
            Some(n)
        }
        Type::And(a, b) | Type::Or(a, b, _) => Some(as_args(u, a, is_param)? + as_args(u, b, is_param)?),
        Type::Annotated(a, _) => as_args(u, a, is_param),
        _ => Some(0),
    }
}
