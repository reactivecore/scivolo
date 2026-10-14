//! Calls of lifted local methods: they call the lifted method with the captured values first; a
//! reference to a local method without parameter lists becomes such a call.

use super::adapt::from_start;
use super::lambda_lift::Function;
use super::lift_names::Caller;
use super::lifted::Method;
use super::local_free::is_method;
use super::walk::children;
use crate::symbols::is_static_module;
use crate::{Lower, Result, unsupported};
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Makes the calls of the local methods among `functions` under `t`, and the references to those
/// without parameter lists, call their `methods` with the captured values first.
pub fn calls(l: &mut Lower, class: SymbolId, t: TNodeId, functions: &[Function], methods: &[Method]) {
    let mut todo = vec![t];
    while let Some(t) = todo.pop() {
        if let Some((i, reference, args)) = call_of(l, t, functions) {
            let span = l.tpd.span(reference);
            let captured: Vec<TNodeId> = functions[i].captured.iter().map(|&s| l.ident(s, span)).collect();
            let home = l.u.symbol(methods[i].sym).owner.unwrap_or(class);
            let receiver = l.this(home, span);
            redirect(l, t, (receiver, &functions[i], &methods[i]), reference, captured, args);
        }
        if !matches!(l.tpd.tree(t), Tree::TypeDef { .. }) {
            todo.extend(children(l.tpd, t));
        }
    }
}

/// Makes the calls of local methods in lifted local class `caller` call their lifted methods,
/// each of `lifted`'s class with its functions and methods, passing a local trait method's
/// parameters standing for the values, else the class's fields in its methods and its
/// constructor's parameters elsewhere.
pub fn calls_from_class(
    l: &mut Lower,
    caller: &Caller,
    lifted: &[(SymbolId, &[Function], &[Method])],
) -> Result<()> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(caller.td) else {
        unreachable!("a class definition")
    };
    for stat in children(l.tpd, template) {
        let in_method = matches!(l.tpd.tree(stat), Tree::DefDef { .. }) && is_method(l, l.tpd.sym(stat));
        let own = l.tpd.sym(stat).and_then(|m| l.trait_methods.get(&m)).map(|(_, pairs)| pairs.clone());
        let mut todo = vec![stat];
        while let Some(t) = todo.pop() {
            if matches!(l.tpd.tree(t), Tree::TypeDef { .. }) {
                continue;
            }
            let found = lifted.iter().find_map(|&(c, fs, ms)| {
                call_of(l, t, fs).map(|(i, reference, args)| (c, &fs[i], &ms[i], reference, args))
            });
            let Some((class, f, method, reference, args)) = found else {
                todo.extend(children(l.tpd, t));
                continue;
            };
            let span = l.tpd.span(reference);
            let receiver = match f.this && !is_static_module(l.u, class) {
                true => {
                    super::outer_fields::outer_path(l, (caller.td, caller.class), class, in_method, span)?
                }
                false => l.this(l.u.symbol(method.sym).owner.unwrap_or(class), span),
            };
            let mut captured = Vec::new();
            for &v in &f.captured {
                let proxies = match &own {
                    Some(pairs) => pairs,
                    None if in_method => &caller.fields,
                    None => &caller.params,
                };
                let Some(&(_, p)) = proxies.iter().find(|&&(w, _)| w == v) else {
                    return unsupported(span, "local classes calling local methods through other methods");
                };
                captured.push(if in_method && own.is_none() {
                    let at = span.end_pos();
                    let this = l.this(caller.class, at);
                    l.select(this, p, at)
                } else {
                    l.ident(p, span)
                });
            }
            redirect(l, t, (receiver, f, method), reference, captured, args);
            todo.extend(children(l.tpd, t));
        }
    }
    Ok(())
}

/// The index among `functions` of the local method `t` calls or refers to without parameter
/// lists, the reference and the arguments.
fn call_of(l: &Lower, t: TNodeId, functions: &[Function]) -> Option<(usize, TNodeId, Vec<TNodeId>)> {
    let (reference, args) = match *l.tpd.tree(t) {
        Tree::Apply { fun, args } => (fun, l.tpd.items(args).to_vec()),
        _ => (t, Vec::new()),
    };
    let i = l.tpd.sym(reference).and_then(|s| functions.iter().position(|f| f.sym == s))?;
    let call = functions[i].closure.is_none()
        && matches!(l.tpd.tree(reference), Tree::Ident { .. })
        && (reference != t || is_parameterless(l, functions[i].def));
    call.then_some((i, reference, args))
}

/// Makes call or reference `t` of function `f` call `method` on `receiver`, with `captured` then
/// `args`.
fn redirect(
    l: &mut Lower,
    t: TNodeId,
    (receiver, f, method): (TNodeId, &Function, &Method),
    reference: TNodeId,
    captured: Vec<TNodeId>,
    args: Vec<TNodeId>,
) {
    let fun = l.select(receiver, method.sym, l.tpd.span(reference));
    make_call(l, t, f.sym, reference, fun, captured.into_iter().chain(args).collect());
}

/// Makes call or reference `t` of method `m` through `reference` apply `fun` to `args`.
pub fn make_call(
    l: &mut Lower,
    t: TNodeId,
    m: SymbolId,
    reference: TNodeId,
    fun: TNodeId,
    args: Vec<TNodeId>,
) {
    let span = l.tpd.span(reference);
    let args = l.tpd.list(&args);
    if reference == t {
        let info = l.u.info(m);
        let result = if let Type::ByName(r) = *l.u.types().get(info) { r } else { info };
        (l.tpd.get_mut(t).sym, l.tpd.get_mut(t).tpe) = (None, Some(result));
        l.tpd.get_mut(t).span = from_start(span);
    }
    l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
}

/// Whether method definition `def` has no parameter lists.
fn is_parameterless(l: &Lower, def: TNodeId) -> bool {
    matches!(*l.tpd.tree(def), Tree::DefDef { paramss, .. } if l.tpd.clause_items(paramss).is_empty())
}
