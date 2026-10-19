// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/TryCatchPatterns.scala
//! dotty's `TryCatchPatterns`: the catch cases from the first one that is not a type test of a
//! simple exception class or a default case become a match on the exception in a last case,
//! rethrowing what no case matches.

use crate::definitions::symbol;
use crate::phases::literals::is_wildcard;
use crate::symbols::is_static;
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::tags::{CASE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{MatchKind, TNodeId, Tree};

/// Makes the cases of `try` `t` from the first that is not a catch case one fallback case with
/// a symbol owned by `owner`; the match in it, if any.
pub fn fallback(l: &mut Lower, t: TNodeId, owner: SymbolId) -> Result<Option<TNodeId>> {
    let Tree::Try { expr, cases, finalizer } = *l.tpd.tree(t) else { return Ok(None) };
    let cases = l.tpd.items(cases).to_vec();
    let mut tried = 0;
    while tried < cases.len() && is_catch_case(l, cases[tried])? {
        tried += 1;
    }
    if tried == cases.len() {
        return Ok(None);
    }
    let span = l.tpd.span(t);
    let throwable = l.u.class_ref("java.lang.Throwable");
    let name = l.fresh.next(l.names, "ex");
    let name = l.names.text(name).to_owned();
    let flags = Flags::default().with(SYNTHETIC).with(CASE);
    let ex = l.u.define(symbol(&name, Kind::Val, owner, flags));
    l.u.set_info(ex, throwable);
    let selector = l.ident(ex, span);
    let thrown = l.ident(ex, Span::NONE);
    let rethrow = l.throw_(thrown);
    let default = l.wildcard(throwable);
    let nothing = l.tpd.tpe(rethrow);
    let rethrow = l.tpd.node(
        Tree::CaseDef { pat: default, guard: None, body: rethrow },
        Span::NONE,
        Some(nothing),
        None,
    );
    let rethrow = l.positioned(rethrow);
    let matched = l.tpd.list(&[&cases[tried..], &[rethrow]].concat());
    let tpe = l.cases_lub(matched);
    let m = l.tpd.node(
        Tree::Match { selector: Some(selector), cases: matched, kind: MatchKind::Plain },
        Span::NONE,
        Some(tpe),
        None,
    );
    let m = l.positioned(m);
    let underscore = l.wildcard(throwable);
    l.tpd.get_mut(underscore).span = span;
    let bind_name = l.term_name(&name);
    let bind =
        l.tpd.node(Tree::Bind { name: bind_name, body: underscore }, Span::NONE, Some(throwable), Some(ex));
    let bind = l.positioned(bind);
    let case = l.tpd.node(Tree::CaseDef { pat: bind, guard: None, body: m }, Span::NONE, Some(tpe), None);
    let case = l.positioned(case);
    let cases = l.tpd.list(&[&cases[..tried], &[case]].concat());
    l.tpd.get_mut(t).tree = Tree::Try { expr, cases, finalizer };
    l.recopy(t);
    Ok(Some(m))
}

/// Whether case `c` is a type test of a simple exception class or a default case, without a
/// guard (dotty's `isCatchCase`).
fn is_catch_case(l: &mut Lower, c: TNodeId) -> Result<bool> {
    let Tree::CaseDef { pat, guard: None, .. } = *l.tpd.tree(c) else { return Ok(false) };
    let unbound = match *l.tpd.tree(pat) {
        Tree::Bind { body, .. } => body,
        _ => pat,
    };
    match *l.tpd.tree(unbound) {
        Tree::Typed { expr, tpt } if is_wildcard(l, expr) => is_simple_throwable(l, tpt),
        _ => Ok(is_wildcard(l, unbound)),
    }
}

/// Whether type tree `tpt` is of a class deriving from `Throwable`, not a trait, its prefix
/// static (dotty's `isSimpleThrowable`).
fn is_simple_throwable(l: &mut Lower, tpt: TNodeId) -> Result<bool> {
    let mut tpe = l.u.dealias(l.tpd.tpe(tpt));
    if let Type::Applied(tycon, _) = *l.u.types().get(tpe) {
        tpe = l.u.dealias(tycon);
    }
    let c = match *l.u.types().get(tpe) {
        Type::Sym(_, c) => c,
        Type::TypeRef(..) => match l.u.class_symbol(tpe) {
            Some(c) => c,
            None => return Ok(false),
        },
        _ => return Ok(false),
    };
    let throwable_tpe = l.u.class_ref("java.lang.Throwable");
    let throwable = l.u.class_symbol(throwable_tpe).expect("`Throwable`");
    let owner = l.u.symbol(c).owner.expect("a symbol's owner");
    let static_prefix =
        l.u.symbol(owner).kind.is_term() || l.u.symbol(owner).kind == Kind::Package || is_static(l.u, owner);
    if l.u.symbol(c).kind != Kind::Class {
        return Ok(static_prefix && l.u.is_subtype(tpe, throwable_tpe));
    }
    Ok(static_prefix && !l.u.is_trait(c) && l.u.derives_from(c, throwable))
}
