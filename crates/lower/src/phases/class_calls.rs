// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LambdaLift.scala

use super::lambda_lift::Function;
use super::lift_names::Caller;
use super::lifted::Method;
use super::lifted_calls::{call_of, push_terms_in, redirect};
use super::local_free::{is_method, original_owner};
use super::walk::{children, is_class_def};
use crate::symbols::is_static_module;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

/// Makes the calls of local methods in lifted local class `caller` and the classes nested in it
/// but `callers` call their lifted methods, each of `lifted`'s class with its functions and
/// methods, passing a local trait method's parameters standing for the values, else the class's
/// fields in its methods and its constructor's parameters elsewhere, else the fields of an
/// enclosing local class (dotty's `proxyRef`).
pub fn calls_from_class(
    l: &mut Lower,
    caller: &Caller,
    callers: &[SymbolId],
    lifted: &[(SymbolId, &[Function], &[Method])],
) -> Result<()> {
    let mut classes = vec![(caller.td, caller.class)];
    while let Some((td, class)) = classes.pop() {
        let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
        for stat in children(l.tpd, template) {
            let in_method = matches!(l.tpd.tree(stat), Tree::DefDef { .. }) && is_method(l, l.tpd.sym(stat));
            let own = l.tpd.sym(stat).and_then(|m| l.trait_methods.get(&m)).map(|(_, pairs)| pairs.clone());
            let mut todo = vec![stat];
            while let Some(t) = todo.pop() {
                if matches!(l.tpd.tree(t), Tree::TypeDef { .. }) {
                    let nested = l.tpd.sym(t).filter(|c| is_class_def(l.tpd, t) && !callers.contains(c));
                    classes.extend(nested.map(|c| (t, c)));
                    continue;
                }
                let found = lifted.iter().find_map(|&(c, fs, ms)| {
                    call_of(l, t, fs).map(|(i, reference, args)| (c, &fs[i], &ms[i], reference, args))
                });
                let Some((home, f, method, reference, args)) = found else {
                    push_terms_in(l, t, &mut todo);
                    continue;
                };
                let span = l.tpd.span(reference);
                let home = l.u.symbol(method.sym).owner.unwrap_or(home);
                let receiver = match f.instance() && !is_static_module(l.u, home) {
                    true => super::outer_fields::outer_path(l, (td, class), home, in_method, span)?,
                    false => l.this(home, span),
                };
                let mut captured = Vec::new();
                for &v in &f.captured {
                    let proxy = match &own {
                        Some(pairs) => pairs.iter().find(|&&(w, _)| w == v).map(|&(_, p)| l.ident(p, span)),
                        None if class != caller.class => None,
                        None => own_proxy(l, caller, v, in_method, span),
                    };
                    let proxy = match proxy {
                        Some(p) => p,
                        None => enclosing_proxy(l, (td, class), v, in_method, span)?,
                    };
                    captured.push(proxy);
                }
                redirect(l, t, (receiver, f, method), reference, captured, args);
                todo.extend(children(l.tpd, t));
            }
        }
    }
    Ok(())
}

/// The field of local class `caller` for value `v` read in a method, else its constructor's
/// parameter, if it has one.
fn own_proxy(l: &mut Lower, caller: &Caller, v: SymbolId, in_method: bool, span: Span) -> Option<TNodeId> {
    let proxies = if in_method { &caller.fields } else { &caller.params };
    let &(_, p) = proxies.iter().find(|&&(w, _)| w == v)?;
    Some(match in_method {
        true => {
            let at = span.end_pos();
            let this = l.this(caller.class, at);
            l.select(this, p, at)
        }
        false => l.ident(p, span),
    })
}

/// The field for value `v` of the innermost local class enclosing `class` that keeps one, read
/// through the enclosing instances.
fn enclosing_proxy(
    l: &mut Lower,
    (td, class): (TNodeId, SymbolId),
    v: SymbolId,
    in_method: bool,
    span: Span,
) -> Result<TNodeId> {
    let at = if in_method { span.end_pos() } else { span };
    let mut outer = original_owner(l, class);
    while let Some(c) = outer {
        if let Some(&field) = l.captured_fields.get(&(c, v)) {
            let path = super::outer_fields::outer_path(l, (td, class), c, in_method, at)?;
            return Ok(l.select(path, field, at));
        }
        outer = original_owner(l, c);
    }
    unsupported(span, "local classes calling local methods through other methods")
}
