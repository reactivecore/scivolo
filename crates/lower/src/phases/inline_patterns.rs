// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/InlinePatterns.scala
//! dotty's `InlinePatterns`: `{ class $anon { def unapply(x) = b }; new $anon() }.unapply(y)`, the
//! placeholder of an inline extractor, becomes `b` with `x` bound to `y`.

use super::beta_reduce::application;
use crate::{Lower, Result};
use std::ops::ControlFlow;
use syntax::ast::Span;
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Template, Tree, Walk, traverse_up};

/// Reduces the placeholders of inline extractors in unit `root`; the class definitions dropped.
pub fn transform(l: &mut Lower, root: TNodeId, owner: SymbolId) -> Result<Vec<TNodeId>> {
    let (mut dropped, mut result) = (Vec::new(), Ok(()));
    traverse_up(
        l,
        root,
        owner,
        &mut |_, _, _| Walk::Children,
        &mut |l, t, owner| match reduce(l, t, owner) {
            Ok(Some(td)) => {
                dropped.push(td);
                ControlFlow::Continue(())
            }
            Ok(None) => ControlFlow::Continue(()),
            Err(e) => {
                result = Err(e);
                ControlFlow::Break(())
            }
        },
    );
    result.map(|()| dropped)
}

/// Makes application `t` in `owner` the body of the placeholder it calls, if it calls one; the
/// placeholder's class definition.
fn reduce(l: &mut Lower, t: TNodeId, owner: SymbolId) -> Result<Option<TNodeId>> {
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return Ok(None) };
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return Ok(None) };
    let Some(name) = l.tpd.sym(fun).map(|s| l.u.symbol(s).name.clone()) else { return Ok(None) };
    let tpe = l.tpd.tpe(t);
    if !matches!(name.as_str(), "unapply" | "unapplySeq")
        || matches!(l.u.types().get(tpe), Type::Method { .. } | Type::Poly { .. })
    {
        return Ok(None);
    }
    let Some((td, def)) = placeholder(l, qualifier, &name) else { return Ok(None) };
    let args = l.tpd.items(args).to_vec();
    let mut bindings = Vec::new();
    let Some(reduced) = application(l, def, &args, owner, &mut bindings)? else { return Ok(None) };
    let reduced = l.ensure_conforms(reduced, tpe);
    let result = if bindings.is_empty() { reduced } else { l.block(&bindings, reduced, Span::NONE) };
    l.copy_node(t, result);
    Ok(Some(td))
}

/// The class definition and its only method `name` of `{ class C { def name(...) }; new C() }`, a
/// class without initialization code.
fn placeholder(l: &Lower, t: TNodeId, name: &str) -> Option<(TNodeId, TNodeId)> {
    let Tree::Block { stats, expr } = *l.tpd.tree(t) else { return None };
    let &[td] = l.tpd.items(stats) else { return None };
    let Template { constr, body, .. } = l.tpd.class_template(td)?;
    let Tree::Apply { fun, .. } = *l.tpd.tree(expr) else { return None };
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return None };
    let no_init = matches!(l.tpd.tree(constr), Tree::DefDef { rhs: None, .. });
    let &[def] = l.tpd.items(body) else { return None };
    let named = matches!(l.tpd.tree(def), Tree::DefDef { .. })
        && l.tpd.sym(def).is_some_and(|s| l.u.symbol(s).name == name);
    (matches!(l.tpd.tree(qualifier), Tree::New { .. }) && no_init && named).then_some((td, def))
}
