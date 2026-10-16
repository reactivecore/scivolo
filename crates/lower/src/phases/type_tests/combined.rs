//! Type tests of unions and intersections: `x.isInstanceOf[A | B]` as `x.isInstanceOf[A] ||
//! x.isInstanceOf[B]`, `&&` for `A & B`, a value other than a name evaluated once into `ev$N`
//! (dotty's `transformTypeTest`).

use super::type_test;
use crate::phases::first_transform::is_pure;
use crate::phases::value_classes::eval_once;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::TypeId;
use typer::tpd::{TNodeId, Tree};

/// `expr` tested for both `parts` of a union if `or`, else of an intersection.
pub fn test(
    l: &mut Lower,
    expr: TNodeId,
    parts: (TypeId, TypeId),
    or: bool,
    in_match: bool,
    span: Span,
) -> Result<TNodeId> {
    if matches!(l.tpd.tree(expr), Tree::Ident { .. }) && is_pure(l, expr) {
        return both(l, expr, parts, or, in_match, span);
    }
    let Some(owner) = l.erasure_owner.filter(|_| !is_pure(l, expr)) else {
        return unsupported(span, "type tests of unions or intersections of paths other than names");
    };
    let (ev, def) = eval_once(l, expr, owner);
    l.tested_values.push((ev, l.tpd.span(expr)));
    let def = l.positioned(def);
    let value = l.ident(ev, Span::NONE);
    let test = both(l, value, parts, or, in_match, span)?;
    let block = l.block(&[def], test, Span::NONE);
    Ok(l.positioned(block))
}

/// Gives the values `test` evaluated once their `ev$N` numbers in the order of the unit's trees,
/// in which dotty's erasure makes them.
pub fn renumber(l: &mut Lower) {
    let mut values = std::mem::take(&mut l.tested_values);
    let mut numbers: Vec<String> = values.iter().map(|&(v, _)| l.u.symbol(v).name.clone()).collect();
    numbers.sort_by_key(|n| n.trim_start_matches("ev$").parse::<u32>().unwrap_or(0));
    values.sort_by_key(|&(_, span)| span.start());
    for ((v, _), name) in values.into_iter().zip(numbers) {
        l.u.rename(v, &name);
    }
}

/// Name `expr` tested for both `parts`.
fn both(
    l: &mut Lower,
    expr: TNodeId,
    parts: (TypeId, TypeId),
    or: bool,
    in_match: bool,
    span: Span,
) -> Result<TNodeId> {
    let Tree::Ident { name } = *l.tpd.tree(expr) else { unreachable!("a name") };
    let left = type_test(l, expr, parts.0, in_match, span)?;
    let n = l.tpd.get(expr);
    let (expr_span, tpe, sym) = (n.span, n.tpe, n.sym);
    let other = l.tpd.node(Tree::Ident { name }, expr_span, tpe, sym);
    let right = type_test(l, other, parts.1, in_match, span)?;
    let boolean = l.u.class_ref("scala.Boolean");
    let boolean = l.u.class_symbol(boolean).expect("`Boolean`");
    let op = if or { "||" } else { "&&" };
    let op = l.u.decl(boolean, op).ok().and_then(|ms| ms.into_iter().next()).expect("a `Boolean` operator");
    Ok(l.call(left, op, &[right]))
}
