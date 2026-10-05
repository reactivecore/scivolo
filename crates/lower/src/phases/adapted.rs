//! The `$adapted` method of a lambda with a primitive parameter or a `Unit` result (dotty's
//! `Erasure.Typer.typedClosure`): its parameters are objects, which erasure then unboxes or
//! casts for the lambda's method.

use super::adapt::from_start;
use super::lifted::Method;
use crate::Lower;
use crate::definitions::symbol;
use tasty::{Erased, Flags, Kind, Type};
use typer::tpd::TNodeId;

/// Whether a function of erased parameters `params` and result `result` has a specialized
/// interface (dotty's `isSpecializableFunctionSAM`).
pub fn specializable(params: &[Erased], result: &Erased) -> bool {
    let prim = |e: &Erased, of: &str| matches!(e, Erased::Prim(c) if of.contains(*c));
    let returns = |of: &str| *result == Erased::Unit || prim(result, of);
    match params {
        [] => returns("zbscijfd"),
        [p] => prim(p, "ijfd") && returns("zijfd"),
        [p, q] => prim(p, "ijd") && prim(q, "ijd") && returns("zijfd"),
        _ => false,
    }
}

/// The `$adapted` method calling `method` with its parameters, then `()` boxed if `adapted`
/// returns an object for a `Unit` result; at the start of `lambda`, of result type tree `tpt`.
pub fn adapted_def(
    l: &mut Lower,
    lambda: TNodeId,
    method: &Method,
    adapted: &Method,
    tpt: TNodeId,
) -> TNodeId {
    let span = from_start(l.tpd.span(lambda));
    let info = l.u.info(adapted.sym);
    let Type::Method { params, result, .. } = l.u.types().get(info).clone() else {
        unreachable!("a method type")
    };
    let mut defs: Vec<TNodeId> = adapted.proxies.iter().map(|&p| l.param_def(p, span)).collect();
    for (name, t) in params.into_iter().skip(adapted.proxies.len()) {
        let p = l.u.define(symbol(&name, Kind::Param, adapted.sym, Flags::default()));
        l.u.set_info(p, t);
        defs.push(l.param_def(p, span));
    }
    let args: Vec<TNodeId> =
        defs.iter().map(|&d| l.ident(l.tpd.sym(d).expect("a parameter"), span)).collect();
    let fun = l.ident(method.sym, span);
    let method_info = l.u.info(method.sym);
    let method_result = l.u.result_type(method_info);
    let mut rhs = l.apply(fun, &args, method_result, span);
    if result != method_result {
        let unit = l.ident(l.defs.boxed_unit_unit, span);
        rhs = l.block(&[rhs], unit, span);
    }
    let tpt = if result != method_result { l.type_tree(result, span) } else { tpt };
    l.method_def(adapted.sym, &defs, tpt, rhs, span)
}
