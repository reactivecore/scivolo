// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PickleQuotes.scala
//! A quote of a literal (dotty's `pickleAsValue`): `'{ 42 }` applied to `quotes` becomes
//! `scala.quoted.ToExpr.IntToExpr[T].apply(42)(quotes)`.

use super::Quoted;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Kind, Type};
use typer::tpd::{TNodeId, Tree};

/// Makes quote application `t` of literal `lit` the call lifting the literal's value.
pub fn lifted(l: &mut Lower, t: TNodeId, q: &Quoted, lit: TNodeId) -> Result<()> {
    let span = l.tpd.span(q.body);
    let Tree::Literal { value } = l.tpd.tree(lit) else { unreachable!("a literal") };
    if matches!(value, Constant::Null | Constant::Unit) {
        return unsupported(span, "quotes of null or ()");
    }
    let lifter = format!("{}ToExpr", value.tag());
    let quoted = l.u.package("scala.quoted").expect("scala.quoted");
    let decls = l.u.decl(quoted, "ToExpr").unwrap_or_default();
    let module = decls.into_iter().find(|&s| l.u.symbol(s).kind == Kind::Val).expect("object ToExpr");
    let class = l.u.module_class(module).expect("ToExpr's class");
    let decls = l.u.decl(class, &lifter).unwrap_or_default();
    let Some(given) = decls.into_iter().find(|&s| l.u.symbol(s).kind.is_term()) else {
        return unsupported(span, format!("quotes without ToExpr.{lifter}"));
    };
    let qual = l.ident(module, Span::NONE);
    let fun = l.select(qual, given, Span::NONE);
    let info = l.u.info(given);
    let Type::Poly { binder, params, result } = l.u.types().get(info).clone() else {
        return unsupported(span, format!("ToExpr.{lifter} not polymorphic"));
    };
    let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
    let instance = l.u.subst(result, binder, &names, &[q.tpe]);
    let targ = l.type_tree(q.tpe, Span::NONE);
    let lifter = l.build().type_apply(fun, &[targ], instance, Span::NONE);
    let lifter_class = l.u.class_symbol(instance).expect("a ToExpr class");
    let apply = l.u.member(lifter_class, "apply")[0];
    let fun = l.select(lifter, apply, Span::NONE);
    let expr = l.u.class_ref("scala.quoted.Expr");
    let expr = l.u.intern_type(Type::Applied(expr, vec![q.tpe]));
    let quotes_type = l.tpd.tpe(q.quotes);
    let binder = l.u.fresh_binder();
    let using = l.u.intern_type(Type::Method {
        binder,
        params: vec![("x$2".into(), quotes_type)],
        result: expr,
        implicit: true,
        contextual: true,
    });
    let first = l.apply(fun, &[lit], using, Span::NONE);
    let call = l.apply(first, &[q.quotes], expr, span);
    let call = l.positioned(call);
    l.copy_node(t, call);
    Ok(())
}
