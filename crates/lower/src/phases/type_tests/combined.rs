//! Type tests of unions and intersections: `x.isInstanceOf[A | B]` as `x.isInstanceOf[A] ||
//! x.isInstanceOf[B]`, `&&` for `A & B` (dotty's `transformTypeTest`).

use super::type_test;
use crate::phases::first_transform::is_pure;
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
    let Tree::Ident { name } = *l.tpd.tree(expr) else {
        return unsupported(span, "type tests of unions or intersections of values other than names");
    };
    if !is_pure(l, expr) {
        return unsupported(span, "type tests of unions or intersections of values other than names");
    }
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
