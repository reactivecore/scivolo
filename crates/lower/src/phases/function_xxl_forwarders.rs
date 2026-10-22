// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/FunctionXXLForwarders.scala
//! dotty's `FunctionXXLForwarders`: a class implementing the `apply` of a function type of more
//! than 22 parameters gets `def apply(args: Array[Object]): Object` calling it with the elements.

use super::bunched::param;
use super::casts::class_named;
use super::function_classes::function_class_arity;
use super::lazy_trees::method;
use crate::definitions::{symbol, type_ref};
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::tags::SYNTHETIC;
use tasty::{Flags, Kind, MAX_IMPLEMENTED_ARITY, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Adds the forwarders to class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    if l.u.is_trait(class) {
        return Ok(());
    }
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { return Ok(()) };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let stats = l.tpd.items(body).to_vec();
    let mut forwarders = Vec::new();
    for &stat in &stats {
        let Tree::DefDef { .. } = l.tpd.tree(stat) else { continue };
        let Some(meth) = l.tpd.sym(stat) else { continue };
        if l.u.symbol(meth).name == "apply" && implements_xxl_apply(l, class, meth) {
            forwarders.push(forwarder(l, class, meth)?);
        }
    }
    if forwarders.is_empty() {
        return Ok(());
    }
    forwarders.extend(stats);
    let body = l.tpd.list(&forwarders);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// The parameter types of method `meth`, its clauses flattened.
fn param_types(l: &mut Lower, meth: SymbolId) -> Vec<TypeId> {
    let mut out = Vec::new();
    let mut t = l.u.info(meth);
    loop {
        match l.u.types().get(t) {
            Type::Poly { result, .. } => t = *result,
            Type::Method { params, result, .. } => {
                out.extend(params.iter().map(|&(_, p)| p));
                t = *result;
            }
            _ => return out,
        }
    }
}

/// Whether `meth` of `class` implements the `apply` of a function class of its number of
/// parameters above the library's arities.
fn implements_xxl_apply(l: &mut Lower, class: SymbolId, meth: SymbolId) -> bool {
    let n = param_types(l, meth).len();
    n > MAX_IMPLEMENTED_ARITY
        && l.u
            .linearization(class)
            .into_iter()
            .any(|b| function_class_arity(l, b).is_some_and(|(a, _)| a as usize == n))
}

/// `def apply(args: Array[Object]): Object = this.meth(args(0).asInstanceOf[T1], …)` in `class`.
fn forwarder(l: &mut Lower, class: SymbolId, meth: SymbolId) -> Result<TNodeId> {
    let (_, array) = param(l);
    let object = type_ref(l.u, l.defs.object);
    let binder = l.u.fresh_binder();
    let params = vec![("args".to_owned(), array)];
    let info =
        l.u.intern_type(Type::Method { binder, params, result: object, implicit: false, contextual: false });
    let sym = l.u.define(symbol("apply", Kind::Def, class, Flags::default().with(SYNTHETIC)));
    l.u.set_info(sym, info);
    let array_class = class_named(l, "scala.Array").expect("`scala.Array`");
    let array_apply = method(l, array_class, "apply", Span::NONE)?;
    let types = param_types(l, meth);
    let info = l.u.info(meth);
    let result = l.u.result_type(info);
    let def = l.forwarding_def(sym, Span::NONE, |l, args, _| {
        let elements: Vec<TNodeId> = types
            .iter()
            .enumerate()
            .map(|(i, &tpe)| {
                let qual = l.copy_ident(args[0]);
                let fun = l.select(qual, array_apply, Span::NONE);
                let index = l.int_literal(i as i32);
                let element = l.apply(fun, &[index], object, Span::NONE);
                l.ensure_conforms(element, tpe)
            })
            .collect();
        let this = l.this(class, Span::NONE);
        let fun = l.select(this, meth, Span::NONE);
        let call = l.apply(fun, &elements, result, Span::NONE);
        l.ensure_conforms(call, object)
    });
    Ok(l.positioned(def))
}
