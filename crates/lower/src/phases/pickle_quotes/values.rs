// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PickleQuotes.scala
//! A quote of a literal (dotty's `pickleAsValue`): `'{ 42 }` applied to `quotes` becomes
//! `scala.quoted.ToExpr.IntToExpr[T].apply(42)(quotes)`, `'{ () }`
//! `quotes.reflect.TreeMethods.asExpr(quotes.reflect.Literal(quotes.reflect.UnitConstant()))`.

use super::Quoted;
use super::reflect::{Reflect, call};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Makes quote application `t` in `owner` of literal `lit` the call lifting the literal's value.
pub fn lifted(l: &mut Lower, t: TNodeId, q: &Quoted, lit: TNodeId, owner: SymbolId) -> Result<()> {
    let span = l.tpd.span(q.body);
    let Tree::Literal { value } = l.tpd.tree(lit) else { unreachable!("a literal") };
    if let Some(constant) = match value {
        Constant::Null => Some("NullConstant"),
        Constant::Unit => Some("UnitConstant"),
        _ => None,
    } {
        let reflect = Reflect { quotes: q.quotes, owner };
        let constant = reflect.method(l, constant, "apply")?;
        let constant = call(l, constant, &[]);
        let literal = reflect.method(l, "Literal", "apply")?;
        let literal = call(l, literal, &[constant]);
        let tpe = l.tpd.tpe(q.body);
        let expr = reflect.as_expr(l, tpe, literal)?;
        l.copy_node(t, expr);
        return Ok(());
    }
    let lifter = format!("{}ToExpr", value.tag());
    let quoted = l.u.package("scala.quoted").expect("scala.quoted");
    let decls = l.u.decls_named(quoted, "ToExpr");
    let module = decls.into_iter().find(|&s| l.u.symbol(s).kind == Kind::Val).expect("object ToExpr");
    let class = l.u.module_class(module).expect("ToExpr's class");
    let decls = l.u.decls_named(class, &lifter);
    let Some(given) = decls.into_iter().find(|&s| l.u.symbol(s).kind.is_term()) else {
        return unsupported(span, format!("quotes without ToExpr.{lifter}"));
    };
    let qual = l.ident(module, Span::NONE);
    let fun = l.select(qual, given, Span::NONE);
    let info = l.u.info(given);
    let Some((binder, names, result)) = l.u.poly(info).map(|p| (p.binder, p.names(), p.result)) else {
        return unsupported(span, format!("ToExpr.{lifter} not polymorphic"));
    };
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
    let first = l.positioned(first);
    let call = l.apply(first, &[q.quotes], expr, Span::NONE);
    let call = l.positioned(call);
    l.copy_node(t, call);
    Ok(())
}
