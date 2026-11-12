// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! The `$adapted` method of a lambda whose parameters or result erase differently from its
//! abstract method's (dotty's `Erasure.adaptClosure`): erasure then adapts the arguments for the
//! lambda's method and its result.

use super::adapt::{adapt, from_start};
use super::lifted::Method;
use super::sam::adaptation_needed;
use crate::Lower;
use crate::definitions::{procedure, specialized, symbol, type_ref};
use syntax::ast::Span;
use tasty::{Erased, Flags, Kind, NameId, SymbolId, TypeId};
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

/// Determines whether class `c` is defined in a source file behaving as the Scala 2 library
/// (dotty's `shouldBehaveAsScala2` of its compilation unit).
pub fn behaves_as_scala2(l: &Lower, c: SymbolId) -> bool {
    let mut top = c;
    while let Some(owner) = l.u.symbol(top).owner.filter(|&o| !l.u.symbol(o).is_package()) {
        top = owner;
    }
    l.u.is_scala2(top)
}

/// The specialized interface a closure of a function type in class `class` implements, its
/// method of erased parameters `params` and result `result` needing adaptations `needed`: a
/// `JFunctionN`, or a `JProcedureN` where only its `Unit` result needs one (dotty's
/// `adaptClosure`).
pub fn specialized_interface(
    l: &mut Lower,
    class: SymbolId,
    needed: (bool, bool),
    (params, result): (&[Erased], &Erased),
) -> Option<Result<SymbolId, String>> {
    match needed {
        (false, false) => None,
        _ if specializable(params, result) && !behaves_as_scala2(l, class) => {
            Some(specialized(l.u, params, result))
        }
        (false, _) if *result == Erased::Unit => Some(procedure(l.u, params.len())),
        _ => None,
    }
}

/// The adaptations a closure of `FunctionN` needs for its method of erased parameters `params`
/// and result `result`.
pub fn function_adaptation(params: &[Erased], result: &Erased) -> (bool, bool) {
    let object = Erased::object();
    adaptation_needed(params, result, &vec![object.clone(); params.len()], &object)
}

/// The `$adapted` method `adapted` of class `class`, of parameters named `names`, calling `anon`,
/// whose last `arity` parameters are a closure's; those of type `Object` where `needed`, and its
/// result too, at `span`.
pub fn erased_adapted_def(
    l: &mut Lower,
    (class, anon, adapted): (SymbolId, SymbolId, SymbolId),
    (arity, needed): (usize, (bool, bool)),
    names: Vec<NameId>,
    span: Span,
) -> TNodeId {
    let info = l.u.info(anon);
    let Some((params, result)) = l.u.method(info).map(|m| (m.params.to_vec(), m.result)) else {
        unreachable!("a method type")
    };
    let erased = l.u.erased_method_value(anon).0;
    let object = type_ref(l.u, l.u.defn().object);
    let first = params.len() - arity;
    let params: Vec<(NameId, TypeId)> = names
        .into_iter()
        .zip(&params)
        .enumerate()
        .map(|(i, (name, &(_, t)))| (name, if needed.0 && i >= first { object } else { t }))
        .collect();
    let adapted_result = if needed.1 { object } else { result };
    let binder = l.u.fresh_binder();
    let method = l.u.intern_method(binder, &params, adapted_result, false, false);
    l.u.set_info(adapted, method);
    l.forwarding_def(adapted, span, |l, args, _| {
        let args: Vec<TNodeId> = args
            .iter()
            .zip(&erased)
            .enumerate()
            .map(|(i, (&a, e))| if needed.0 && i >= first { adapt(l, a, e) } else { a })
            .collect();
        let this = l.this(class, span);
        let fun = l.select(this, anon, span);
        let call = l.apply(fun, &args, result, span);
        if needed.1 { adapt(l, call, &Erased::object()) } else { call }
    })
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
) -> (Vec<(NameId, TypeId)>, TypeId) {
    let (erased, erased_result) = l.u.erased_method_value(sam);
    let info = l.u.info(sam);
    let declared = l.u.method(info).map(|m| m.params.to_vec()).unwrap_or_default();
    let declared_result = l.u.result_type(info);
    let object = type_ref(l.u, l.u.defn().object);
    let as_erased = |e: &Erased, t: TypeId| if *e == Erased::object() { object } else { t };
    let mut out: Vec<(NameId, TypeId)> = params.iter().map(|&(p, t)| (l.u.interned_name(p), t)).collect();
    if params_needed {
        for ((param, (name, t)), e) in out.iter_mut().zip(declared).zip(&erased) {
            *param = (if result_needed { name } else { param.0 }, as_erased(e, t));
        }
    }
    let result = if result_needed { as_erased(&erased_result, declared_result) } else { result };
    (out, result)
}

/// The `$adapted` method calling `method` with its parameters, then `()` boxed if `adapted`
/// returns an object for a `Unit` result; at the start of the lambda's `closure`, defined where
/// the closure was in another source if it was, of result type tree `tpt`.
pub fn adapted_def(
    l: &mut Lower,
    closure: TNodeId,
    method: &Method,
    adapted: &Method,
    tpt: TNodeId,
) -> TNodeId {
    let span = from_start(l.tpd.span(closure));
    let info = l.u.info(adapted.sym);
    let Some((params, result)) = l.u.method(info).map(|m| (m.params.to_vec(), m.result)) else {
        unreachable!("a method type")
    };
    let mut defs: Vec<TNodeId> = adapted.proxies.iter().map(|&p| l.param_def(p, span)).collect();
    for (name, t) in params.into_iter().skip(adapted.proxies.len()) {
        let name = l.u.name(name).to_owned();
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
    let def_span = from_start(super::drop_inlined::own_span(l, closure));
    l.method_def(adapted.sym, &defs, tpt, rhs, def_span)
}
