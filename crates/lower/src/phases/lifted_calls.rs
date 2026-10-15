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
use typer::tpd::{List, TNodeId, Tree};

/// Makes the calls of the local methods among `functions` in definition `td` of `class`, and the
/// references to those without parameter lists, call their `methods` with the captured values
/// first, those narrowed into an enclosing class from outside them through the enclosing
/// instance.
pub fn calls(
    l: &mut Lower,
    (td, class): (TNodeId, SymbolId),
    functions: &[Function],
    methods: &[Method],
) -> Result<()> {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    for stat in children(l.tpd, template) {
        let in_method = matches!(l.tpd.tree(stat), Tree::DefDef { .. }) && is_method(l, l.tpd.sym(stat));
        let mut todo = vec![(stat, None)];
        while let Some((t, around)) = todo.pop() {
            if let Some((i, reference, args)) = call_of(l, t, functions) {
                let span = l.tpd.span(reference);
                let captured: Vec<TNodeId> =
                    functions[i].captured.iter().map(|&s| l.ident(s, span)).collect();
                let home = l.u.symbol(methods[i].sym).owner.unwrap_or(class);
                let receiver = match functions[i].into {
                    Some(c) if around == Some(c) || c == class => l.this(c, span),
                    Some(_) if around.is_some() => {
                        return unsupported(span, "local functions narrowed into different classes");
                    }
                    Some(c) => super::outer_fields::outer_path(l, (td, class), c, in_method, span)?,
                    None => l.this(home, span),
                };
                redirect(l, t, (receiver, &functions[i], &methods[i]), reference, captured, args);
            }
            if matches!(l.tpd.tree(t), Tree::TypeDef { .. }) {
                continue;
            }
            let around = functions.iter().find(|f| f.def == t).map_or(around, |f| f.into);
            let mut next = Vec::new();
            push_terms_in(l, t, &mut next);
            todo.extend(next.into_iter().map(|c| (c, around)));
        }
    }
    Ok(())
}

/// Pushes the children of `t` but the method a return returns from onto `todo`.
fn push_terms_in(l: &Lower, t: TNodeId, todo: &mut Vec<TNodeId>) {
    match *l.tpd.tree(t) {
        Tree::Return { expr, .. } => todo.push(expr),
        _ => todo.extend(children(l.tpd, t)),
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
                push_terms_in(l, t, &mut todo);
                continue;
            };
            let span = l.tpd.span(reference);
            let home = l.u.symbol(method.sym).owner.unwrap_or(class);
            let receiver = match f.instance() && !is_static_module(l.u, home) {
                true => super::outer_fields::outer_path(l, (caller.td, caller.class), home, in_method, span)?,
                false => l.this(home, span),
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
/// lists, the reference past type arguments, and the arguments.
fn call_of(l: &Lower, t: TNodeId, functions: &[Function]) -> Option<(usize, TNodeId, Vec<TNodeId>)> {
    let (fun, args) = match *l.tpd.tree(t) {
        Tree::Apply { fun, args } => (fun, l.tpd.items(args).to_vec()),
        _ => (t, Vec::new()),
    };
    let reference = match *l.tpd.tree(fun) {
        Tree::TypeApply { fun, .. } => fun,
        _ => fun,
    };
    let i = l.tpd.sym(reference).and_then(|s| functions.iter().position(|f| f.sym == s))?;
    let call = functions[i].closure.is_none()
        && matches!(l.tpd.tree(reference), Tree::Ident { .. })
        && (fun != t || is_parameterless(l, functions[i].def));
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
    if !matches!(l.tpd.tree(t), Tree::Apply { .. }) {
        let info = super::lifted::value_info(l, m);
        let result = if let Type::ByName(r) = *l.u.types().get(info) { r } else { info };
        (l.tpd.get_mut(t).sym, l.tpd.get_mut(t).tpe) = (None, Some(result));
        l.tpd.get_mut(t).span = from_start(span);
        l.applied_references.insert(t);
    }
    l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
}

/// Whether method definition `def` has no value parameter lists.
fn is_parameterless(l: &Lower, def: TNodeId) -> bool {
    let Tree::DefDef { paramss, .. } = *l.tpd.tree(def) else { return false };
    let type_clause =
        |&c: &List| l.tpd.items(c).first().is_some_and(|&p| matches!(l.tpd.tree(p), Tree::TypeDef { .. }));
    l.tpd.clause_items(paramss).iter().all(type_clause)
}
