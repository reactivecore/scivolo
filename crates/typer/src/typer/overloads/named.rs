// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Whether an alternative applies to named arguments, reordered by its parameter names (dotty's
//! `ApplicableToTypes` with `reorder`).

use super::applicability::{applicable, applicable_slots};
use crate::ctx::Ctx;
use crate::typer::Typer;
use tasty::{SymbolId, Type, TypeId};

/// Checks whether `alt`, seen from `prefix`, applies to the arguments of `proto` of types
/// `arg_types`, named ones at the parameters of their names, as `f(y = 1, x = "a")` to
/// `def f(x: String, y: Int)`; `Some(false)` if the names do not fit, `None` for more arguments
/// than parameters of a repeated one.
pub fn applicable_to_proto(
    t: &mut Typer,
    alt: SymbolId,
    prefix: Option<TypeId>,
    (proto, arg_types): (usize, &[TypeId]),
    views: bool,
    ctx: Ctx,
) -> Option<bool> {
    let args = t.protos[proto].args.clone();
    if args.iter().all(|&a| t.arg_name(a).is_none()) {
        return applicable(t, alt, prefix, arg_types, views, ctx);
    }

    let tpe = t.u.intern_type(Type::Sym(prefix, alt));
    let info = t.method_info(tpe)?;
    let info = t.u.strip_poly(info);
    let Some(params) = t.u.method(info).map(|m| m.params.to_vec()) else { return Some(false) };
    let repeated = params.last().is_some_and(|&(_, p)| t.repeated_param_elem(p).is_some());
    if repeated && args.len() > params.len() {
        return None;
    }

    let names: Vec<String> = params.into_iter().map(|(n, _)| t.u.name(n).to_owned()).collect();
    let Ok(aligned) = t.ordered_args(&args, &names) else { return Some(false) };
    let slots: Vec<Option<TypeId>> = aligned
        .iter()
        .map(|slot| slot.and_then(|a| args.iter().position(|&b| b == a)).map(|i| arg_types[i]))
        .collect();
    applicable_slots(t, alt, prefix, &slots, views, ctx)
}
