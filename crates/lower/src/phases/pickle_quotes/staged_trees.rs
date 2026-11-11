// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Splicing.scala
//! The splices and quotes `Splicing` makes, as the typer gives them: `Expr.splice[T](closure)`
//! and `Expr.quote[T](body).apply(quotes)`.

use super::avoid::avoided;
use crate::Lower;
use crate::definitions::symbol;
use names::{name_kinds, nme};
use syntax::ast::Span;
use tasty::tags::SYNTHETIC;
use tasty::{Flags, Kind, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// `Expr.splice[T]((x$0: Quotes) ?=> body)` in `owner`, `body` made from a reference to `x$0`
/// in the closure's method (dotty's `spliced`); `T` is `tpe` as its hole's type avoids it.
pub fn spliced(
    l: &mut Lower,
    tpe: TypeId,
    owner: SymbolId,
    body: impl FnOnce(&mut Lower, TNodeId, SymbolId) -> TNodeId,
) -> TNodeId {
    let class = l.u.class_ref(tasty::defn::QUOTED_EXPR);
    let expr = l.u.intern_applied(class, &[tpe]);
    let quotes = l.u.class_ref(tasty::defn::QUOTES);
    let function = l.u.class_ref(tasty::defn::CONTEXT_FUNCTION1);
    let function = l.u.intern_applied(function, &[quotes, expr]);
    let anon = l.u.define(symbol(nme::ANON_FUN, Kind::Def, owner, Flags::default().with(SYNTHETIC)));
    let binder = l.u.fresh_binder();
    let params = [(l.u.intern_name(&name_kinds::synthetic_param(0)), quotes)];
    let info = l.u.intern_method(binder, &params, expr, false, true);
    l.u.set_info(anon, info);
    let def = l.forwarding_def(anon, Span::NONE, |l, args, _| body(l, args[0], anon));
    let def = l.positioned(def);
    let meth = l.ident(anon, Span::NONE);
    let env = l.tpd.list(&[]);
    let closure = l.tpd.node(Tree::Closure { env, meth, tpt: None }, Span::NONE, Some(function), None);
    let closure = l.block(&[def], closure, Span::NONE);
    let closure = l.positioned(closure);
    let avoided = avoided(l.u, tpe);
    let fun = runtime(l, "splice", avoided);
    let splice = l.apply(fun, &[closure], avoided, Span::NONE);
    l.positioned(splice)
}

/// `Expr.quote[T](body).apply(quotes)`, `T` the type of `body`.
pub fn quoted(l: &mut Lower, body: TNodeId, quotes: TNodeId) -> TNodeId {
    let tpe = l.tpd.tpe(body);
    let class = l.u.class_ref(tasty::defn::QUOTED_EXPR);
    let expr = l.u.intern_applied(class, &[tpe]);
    let quotes_type = l.u.class_ref(tasty::defn::QUOTES);
    let function = l.u.class_ref(tasty::defn::CONTEXT_FUNCTION1);
    let function = l.u.intern_applied(function, &[quotes_type, expr]);
    let fun = runtime(l, "quote", tpe);
    let quote = l.apply(fun, &[body], function, Span::NONE);
    let quote = l.positioned(quote);
    let class = l.u.class_symbol(function).expect("ContextFunction1");
    let apply = l.u.member(class, nme::APPLY)[0];
    let fun = l.select(quote, apply, Span::NONE);
    let fun = l.positioned(fun);
    let applied = l.apply(fun, &[quotes], expr, Span::NONE);
    l.positioned(applied)
}

/// `scala.quoted.runtime.Expr.method[tpe]`.
fn runtime(l: &mut Lower, method: &str, tpe: TypeId) -> TNodeId {
    let runtime = l.u.class_ref("scala.quoted.runtime.Expr$");
    let runtime = l.u.class_symbol(runtime).expect("scala.quoted.runtime.Expr");
    let method = l.u.decl(runtime, method).ok().and_then(|d| d.first().copied()).expect("a staging method");
    let fun = l.ident(method, Span::NONE);
    let targ = l.type_tree(tpe, Span::NONE);
    l.build().type_apply(fun, &[targ], tpe, Span::NONE)
}
