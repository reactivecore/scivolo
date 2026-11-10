// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Whether an argument fits a parameter (dotty's `isCompatible`): by subtyping and numeric
//! widening, closures by their single abstract method, and with implicit conversions.

use crate::ctx::Ctx;
use crate::tpd::Tree as T;
use crate::typer::Typer;
use crate::typer::implicits::Goal;
use tasty::{MAX_IMPLEMENTED_ARITY, TypeId};

/// Checks whether an argument of type `arg` fits a parameter of type `param` in `ctx`, through
/// an implicit conversion if `views` (dotty's `isCompatible`, else its direct applicability):
/// `None` if undecided.
pub fn compatible(t: &mut Typer, arg: TypeId, param: TypeId, views: bool, ctx: Ctx) -> Option<bool> {
    if t.conforms(arg, param) {
        return Some(true);
    }
    match sam_arg_ok(t, arg, param, views) {
        Ok(Some(fits)) => return Some(fits),
        Ok(None) => {}
        Err(_) => return None,
    }
    if let (Some(a), Some(p)) = (t.value_class(arg), t.value_class(param))
        && crate::typer::widening::widens(&a, &p)
    {
        return Some(true);
    }
    if !views {
        return Some(false);
    }
    view_exists(t, arg, param, ctx)
}

/// Checks whether an implicit conversion takes a value of type `arg` to `param`, or `arg` is a
/// named tuple whose values conform to `param`, a `Tuple` (dotty's `viewExists`): `None` if the
/// search is outside the subset.
fn view_exists(t: &mut Typer, arg: TypeId, param: TypeId, ctx: Ctx) -> Option<bool> {
    let tuple = t.resolve_full_name(crate::defn::TUPLE, true);
    if let (Some(values), Some(tuple)) = (t.stripped_named_tuple(arg), tuple)
        && t.u.base_type(param, tuple).is_some()
        && t.conforms(values, param)
    {
        return Some(true);
    }

    let (before, unsupported) = (t.constraint.attempt(), t.unsupported.clone());
    let dummy = t.tpd.node(T::TypeTree { inferred: true }, syntax::ast::Span::NONE, Some(arg), None);
    let found = t.converted(dummy, Goal::Conform(param), ctx);
    let clean = t.unsupported == unsupported;
    t.constraint.abandon(before);
    t.unsupported = unsupported;
    match found {
        Ok(found) if clean => Some(found.is_some()),
        _ => None,
    }
}

/// Checks whether an argument of function type `arg` fits single abstract method type `param`:
/// `arg` conforms to the function type of the method (dotty's `SAMargOK`); `None` if `arg` is no
/// function type or `param` no such type, or a `PartialFunction` unless `views`.
fn sam_arg_ok(t: &mut Typer, arg: TypeId, param: TypeId, views: bool) -> Result<Option<bool>, &'static str> {
    let arg = t.u.widen(arg);
    let Some(n) = (0..=MAX_IMPLEMENTED_ARITY).find(|&n| t.function_parts(arg, n).is_some()) else {
        return Ok(None);
    };
    if t.function_parts(param, n).is_some() || !views && t.is_partial_function(param) {
        return Ok(None);
    }
    let Some(sam) = t.sam_shape(param, n)? else { return Ok(None) };
    let class = t.function_class(n).ok_or("function class")?;
    let mut args = sam.params;
    args.push(sam.result);
    let fun = t.u.intern_applied(class, &args);
    Ok(Some(t.conforms(arg, fun)))
}
