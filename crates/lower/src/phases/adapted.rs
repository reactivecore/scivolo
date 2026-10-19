// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! The `$adapted` method of a lambda whose parameters or result erase differently from its
//! abstract method's (dotty's `Erasure.adaptClosure`): erasure then adapts the arguments for the
//! lambda's method and its result.

use super::adapt::from_start;
use super::lifted::Method;
use crate::Lower;
use crate::definitions::{symbol, type_ref};
use syntax::ast::Span;
use tasty::{Erased, Flags, Kind, SymbolId, Type, TypeId};
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

/// The parameters and result type of the `$adapted` method of a lambda with `params` and
/// `result`: the types of abstract method `sam` where `needed`, its parameter names where both
/// are, a type erased to `Object` as `Object`.
pub fn adapted_types(
    l: &mut Lower,
    sam: SymbolId,
    (params_needed, result_needed): (bool, bool),
    params: &[(SymbolId, TypeId)],
    result: TypeId,
) -> (Vec<(String, TypeId)>, TypeId) {
    let (erased, erased_result) = l.u.erased_method_value(sam);
    let info = l.u.info(sam);
    let declared = match l.u.types().get(info) {
        Type::Method { params, .. } => params.clone(),
        _ => Vec::new(),
    };
    let declared_result = l.u.result_type(info);
    let object = type_ref(l.u, l.defs.object);
    let as_erased =
        |e: &Erased, t: TypeId| if *e == Erased::Class("java.lang.Object".into()) { object } else { t };
    let mut out: Vec<(String, TypeId)> =
        params.iter().map(|&(p, t)| (l.u.symbol(p).name.clone(), t)).collect();
    if params_needed {
        for ((param, (name, t)), e) in out.iter_mut().zip(declared).zip(&erased) {
            *param = (if result_needed { name } else { param.0.clone() }, as_erased(e, t));
        }
    }
    let result = if result_needed { as_erased(&erased_result, declared_result) } else { result };
    (out, result)
}

/// The `$adapted` method calling `method` with its parameters, then `()` boxed if `adapted`
/// returns an object for a `Unit` result; at the start of the lambda's `closure`, of result type
/// tree `tpt`.
pub fn adapted_def(
    l: &mut Lower,
    closure: TNodeId,
    method: &Method,
    adapted: &Method,
    tpt: TNodeId,
) -> TNodeId {
    let span = from_start(l.tpd.span(closure));
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
        defs.iter().map(|&d| l.ident(l.tpd.sym(d).expect("a parameter"), Span::NONE)).collect();
    let fun = l.ident(method.sym, span);
    let method_info = l.u.info(method.sym);
    let method_result = l.u.result_type(method_info);
    let mut rhs = l.apply(fun, &args, method_result, span);
    if result != method_result && l.u.erase(method_result) == Erased::Unit {
        let unit = l.ident(l.defs.boxed_unit_unit, span);
        rhs = l.block(&[rhs], unit, span);
    }
    let tpt = if result != method_result { l.type_tree(result, span) } else { tpt };
    l.method_def(adapted.sym, &defs, tpt, rhs, span)
}
