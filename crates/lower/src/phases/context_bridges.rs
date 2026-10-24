// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Bridges.scala
//! Bridges of methods flattened by `ContextFunctionResults` overriding abstract methods that
//! were not: the bridge returns a closure taking the context parameters (dotty's `etaExpand`).

use super::adapt::adapt;
use super::lift_names::fresh;
use crate::definitions::symbol;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{ARTIFACT, CASEaccessor, FIELDaccessor, LAZY, OBJECT, PARAMsetter, PRIVATE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// `def m(params): F = closure(m$$anonfun$N)` of the type of `other`, and the closure's method
/// `def m$$anonfun$N(params, contextParams) = this.m(params, contextParams)`, in `class` at `span`.
pub fn bridge(
    l: &mut Lower,
    class: SymbolId,
    m: SymbolId,
    other: SymbolId,
    span: Span,
) -> Result<Vec<TNodeId>> {
    let (_, depth) = l.context_results[&m];
    let (m_info, other_info) = (l.u.info(m), l.u.info(other));
    let own_clauses = clauses(l, m_info);
    let own = own_clauses.len() - depth;
    if depth != 1 || clauses(l, other_info).len() != own {
        return unsupported(span, "bridges of methods with context function results flattened otherwise");
    }
    let m_result = l.u.result_type(m_info);
    let other_params: Vec<(String, TypeId)> = clauses(l, other_info).concat();
    let mut captured: Vec<(String, TypeId)> = own_clauses[..own].concat();
    for ((name, tpe), (_, other)) in captured.iter_mut().zip(&other_params) {
        *name = fresh(l, name);
        *tpe = *other;
    }
    let (erased, others) = (l.u.erased_method(m).0, l.u.erased_method(other).0);
    let value_params = l.u.erased_method_value(m).0;
    let context = own_clauses[own].clone();
    let arity = context.len();
    let flags = Flags::default().with(PRIVATE).with(SYNTHETIC).with(ARTIFACT);
    let anon_name = fresh(l, &format!("{}$$anonfun", l.u.symbol(m).name));
    let anon = l.u.define(symbol(&anon_name, Kind::Def, class, flags));
    let binder = l.u.fresh_binder();
    let params = captured.into_iter().chain(context).collect();
    let info = l.u.intern_type(Type::Method {
        binder,
        params,
        result: m_result,
        implicit: false,
        contextual: false,
    });
    l.u.set_info(anon, info);
    let anon_def = l.forwarding_def(anon, span, |l, args, result| {
        let args: Vec<TNodeId> = args
            .iter()
            .enumerate()
            .map(|(i, &a)| match (erased.get(i), others.get(i), value_params.get(i)) {
                (Some(p), Some(o), Some(v)) if p != o => adapt(l, a, v),
                _ => a,
            })
            .collect();
        let args = &args[..];
        let this = l.this(class, span);
        let fun = l.select(this, m, span);
        l.apply(fun, args, result, span)
    });
    let mut flags = l.u.symbol(m).flags;
    for tag in [FIELDaccessor, PARAMsetter, CASEaccessor, LAZY, OBJECT] {
        flags = flags.without(tag);
    }
    let info = l.u.info(other);
    let (sym, def) = l.method_like(class, m, flags, info, span, |l, args, _| {
        let function = l.u.class_ref(&format!("scala.Function{arity}"));
        let env = l.tpd.list(args);
        let meth = l.ident(anon, span);
        l.tpd.node(Tree::Closure { env, meth, tpt: None }, span, Some(function), None)
    })?;
    l.bridges.insert(sym);
    Ok(vec![def, anon_def])
}

/// Whether a method of class `owner` flattened by `ContextFunctionResults` gets a bridge
/// overriding `d` (were it not flattened): an abstract method of the unit `owner` inherits.
pub fn bridged(l: &mut Lower, owner: SymbolId, d: SymbolId) -> bool {
    let deferred = l.u.is_deferred(d) && l.u.symbol(d).tree.is_none();
    deferred && l.u.symbol(d).owner.is_some_and(|o| o != owner && l.u.derives_from(owner, o))
}

/// The parameter clauses of method type `t`, each parameter with its name.
pub fn clauses(l: &Lower, mut t: TypeId) -> Vec<Vec<(String, TypeId)>> {
    let mut out = Vec::new();
    loop {
        match l.u.types().get(t) {
            Type::Poly { result, .. } => t = *result,
            Type::Method { params, result, .. } => {
                out.push(params.clone());
                t = *result;
            }
            _ => return out,
        }
    }
}
