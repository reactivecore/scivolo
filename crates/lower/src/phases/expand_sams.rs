//! Closures of `PartialFunction` types as anonymous classes overriding `isDefinedAt` and
//! `applyOrElse` with the closure's cases (dotty's `ExpandSAMs.toPartialFunction`); closures of
//! other single abstract method types that are not functional interfaces as anonymous classes
//! too, those of functional interfaces stay closures.

use super::anon_class::{class_symbol, def, instance, override_sym, param, wildcard};
use super::lambda_lift::lambda;
use super::sam::is_platform_sam;
use super::sam_classes::to_anon_class;
use super::tree_copy::{Copier, copy};
use super::walk::subtrees;
use crate::definitions::{PartialFunctions, applied, symbol, type_ref};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use syntax::ast::tree::MatchKind;
use tasty::tags::{CASE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// A closure of a `PartialFunction` type: its block, its method's definition and symbol, and its
/// type.
struct Closure {
    block: TNodeId,
    def: TNodeId,
    anon: SymbolId,
    sam: TypeId,
}

/// What both methods are made from: the closure's method and parameter, the match of its body,
/// and the closure's span.
struct Expansion {
    anon: SymbolId,
    param: SymbolId,
    rhs: TNodeId,
    span: Span,
}

/// Expands the closures of `PartialFunction` types and of other single abstract method types
/// that are not functional interfaces in unit `root`; whether there were any.
pub fn transform(l: &mut Lower, root: TNodeId) -> Result<bool> {
    let mut found = Vec::new();
    let mut classes = false;
    for t in subtrees(l.tpd, root) {
        let Some((def, closure, anon)) = lambda(l, t) else { continue };
        let Tree::Closure { tpt: Some(tpt), .. } = *l.tpd.tree(closure) else { continue };
        let sam = l.tpd.tpe(tpt);
        let dealiased = l.u.dealias(sam);
        let Some(class) = l.u.class_symbol(dealiased) else { continue };
        if class == l.partial_functions()?.class {
            found.push(Closure { block: t, def, anon, sam });
        } else if !is_platform_sam(l, class, l.tpd.span(t))? {
            to_anon_class(l, t, def, anon, dealiased)?;
            classes = true;
        }
    }
    for c in &found {
        let inner = subtrees(l.tpd, c.def);
        if found.iter().any(|d| inner.contains(&d.block)) {
            return unsupported(l.tpd.span(c.block), "partial functions in partial functions");
        }
    }
    for c in &found {
        to_partial_function(l, c)?;
    }
    Ok(classes || !found.is_empty())
}

fn to_partial_function(l: &mut Lower, c: &Closure) -> Result<()> {
    let span = l.tpd.span(c.block);
    let Tree::DefDef { paramss, rhs: Some(rhs), .. } = *l.tpd.tree(c.def) else {
        unreachable!("a closure's method")
    };
    let params: Vec<TNodeId> =
        l.tpd.clause_items(paramss).iter().flat_map(|&p| l.tpd.items(p).to_vec()).collect();
    let [param] = params[..] else { return unsupported(span, "partial functions of several parameters") };
    let param = l.tpd.sym(param).expect("a parameter's symbol");
    let pf = l.partial_functions()?;
    let info = l.u.info(c.anon);
    let Type::Method { result, .. } = *l.u.types().get(info) else { unreachable!("a closure's method type") };
    let arg = l.u.info(param);
    let parents = vec![applied(l.u, pf.abstract_class, vec![arg, result]), type_ref(l.u, pf.serializable)];
    let owner = l.u.symbol(c.anon).owner.expect("a closure's owner");
    let cls = class_symbol(l, owner, parents);
    let rhs = partial_fun_rhs(l, rhs, param);
    let x = Expansion { anon: c.anon, param, rhs, span };
    let is_defined_at = is_defined_at_def(l, &x, cls, &pf, c.sam)?;
    let apply_or_else = apply_or_else_def(l, &x, cls, &pf, c.sam)?;
    let block = instance(l, cls, pf.abstract_ctor, &[is_defined_at, apply_or_else], Span::NONE);
    l.copy_node(c.block, block);
    l.u.unenter(c.anon);
    Ok(())
}

/// The match `rhs` is, else `param match { case _ => rhs }` (dotty's `partialFunRHS`).
fn partial_fun_rhs(l: &mut Lower, rhs: TNodeId, param: SymbolId) -> TNodeId {
    match *l.tpd.tree(rhs) {
        Tree::Match { selector: Some(_), .. } => return rhs,
        Tree::Block { stats, expr } if l.tpd.items(stats).is_empty() => {
            return partial_fun_rhs(l, expr, param);
        }
        _ => {}
    }
    let info = l.u.info(param);
    let wildcard = wildcard(l, info);
    let tpe = l.tpd.tpe(rhs);
    let case =
        l.tpd.node(Tree::CaseDef { pat: wildcard, guard: None, body: rhs }, Span::NONE, Some(tpe), None);
    l.positioned(case);
    let selector = l.ident(param, Span::NONE);
    let cases = l.tpd.list(&[case]);
    let m = Tree::Match { selector: Some(selector), cases, kind: MatchKind::Plain };
    let m = l.tpd.node(m, Span::NONE, Some(tpe), None);
    l.positioned(m)
}

fn is_defined_at_def(
    l: &mut Lower,
    x: &Expansion,
    cls: SymbolId,
    pf: &PartialFunctions,
    sam: TypeId,
) -> Result<TNodeId> {
    let (sym, _) = override_sym(l, cls, pf.is_defined_at, sam);
    let info = l.u.info(sym);
    let Type::Method { params, result, .. } = l.u.types().get(info).clone() else {
        unreachable!("a method type")
    };
    let p = param(l, sym, &params[0].0, params[0].1);
    let default = l.build().literal(Constant::Boolean(false), Span::NONE);
    let rhs = translate_match(l, x, (sym, p), default, result, true)?;
    Ok(def(l, sym, (&[], &[p]), (result, rhs), x.span.start_pos()))
}

fn apply_or_else_def(
    l: &mut Lower,
    x: &Expansion,
    cls: SymbolId,
    pf: &PartialFunctions,
    sam: TypeId,
) -> Result<TNodeId> {
    let (sym, tsyms) = override_sym(l, cls, pf.apply_or_else, sam);
    let info = l.u.info(sym);
    let Type::Poly { result, .. } = *l.u.types().get(info) else { unreachable!("a polymorphic method type") };
    let Type::Method { params, result, .. } = l.u.types().get(result).clone() else {
        unreachable!("a method type")
    };
    let vparams: Vec<SymbolId> = params.iter().map(|(name, t)| param(l, sym, name, *t)).collect();
    let default = l.ident(vparams[1], Span::NONE);
    let apply = l.select(default, pf.function1_apply, Span::NONE);
    l.positioned(apply);
    let arg = l.ident(vparams[0], Span::NONE);
    let call = l.apply(apply, &[arg], result, Span::NONE);
    l.positioned(call);
    let rhs = translate_match(l, x, (sym, vparams[0]), call, result, false)?;
    Ok(def(l, sym, (&tsyms, &vparams), (result, rhs), x.span.start_pos()))
}

/// The closure's match copied into `method` with `pf_param` for the closure's parameter, its
/// cases' bodies `true` if `defined`, and a last case of `default` unless one matches anything
/// (dotty's `translateMatch`).
fn translate_match(
    l: &mut Lower,
    x: &Expansion,
    (method, pf_param): (SymbolId, SymbolId),
    default: TNodeId,
    tpe: TypeId,
    defined: bool,
) -> Result<TNodeId> {
    let syms = std::iter::once((x.param, pf_param)).collect();
    let copied = copy(l, &mut Copier { syms, from: x.anon, to: method }, x.rhs)?;
    let Tree::Match { selector: Some(selector), cases, kind } = *l.tpd.tree(copied) else {
        unreachable!("a match")
    };
    let mut cases = l.tpd.items(cases).to_vec();
    if defined {
        let mut at = Span::NONE;
        for c in &mut cases {
            (*c, at) = defined_case(l, *c, at)?;
        }
    }
    let mut tpe = tpe;
    if !cases.iter().any(|&c| is_default_case(l, c)) {
        let selector_tpe = l.tpd.tpe(selector);
        let selector_tpe = l.u.widen(selector_tpe);
        cases.push(default_case(l, method, selector_tpe, default));
    } else if !defined {
        tpe = l.tpd.tpe(copied);
    }
    let span = l.tpd.span(copied);
    let cases = l.tpd.list(&cases);
    let m = l.tpd.node(Tree::Match { selector: Some(selector), cases, kind }, Span::NONE, Some(tpe), None);
    Ok(l.with_span(m, span))
}

/// Case `c` with the body `true`, at `at` unless that is none, and the span the body got: all
/// cases share one literal (dotty's `translateCase`).
fn defined_case(l: &mut Lower, c: TNodeId, at: Span) -> Result<(TNodeId, Span)> {
    let Tree::CaseDef { pat, guard, body } = *l.tpd.tree(c) else { unreachable!("a case") };
    if matches!(l.tpd.tree(body), Tree::Match { kind: MatchKind::Sub, .. }) {
        return unsupported(l.tpd.span(body), "sub matches in partial functions");
    }
    let tru = l.build().literal(Constant::Boolean(true), at);
    let tpe = l.tpd.tpe(tru);
    let case = l.tpd.node(Tree::CaseDef { pat, guard, body: tru }, Span::NONE, Some(tpe), None);
    let case = l.with_span(case, l.tpd.span(c));
    Ok((case, l.tpd.span(tru)))
}

/// Whether case `c` has no guard and a pattern matching anything (dotty's `isDefaultCase`).
fn is_default_case(l: &Lower, c: TNodeId) -> bool {
    let Tree::CaseDef { mut pat, guard: None, .. } = *l.tpd.tree(c) else { return false };
    while let Tree::Bind { body, .. } = *l.tpd.tree(pat) {
        pat = body;
    }
    matches!(*l.tpd.tree(pat), Tree::Ident { name } if l.names.text(name.name) == "_")
}

/// `case _ @ _ => value` of method `method` for a selector of type `tpe`.
fn default_case(l: &mut Lower, method: SymbolId, tpe: TypeId, value: TNodeId) -> TNodeId {
    let sym = l.u.define(symbol("_", Kind::Val, method, Flags::default().with(SYNTHETIC).with(CASE)));
    l.u.set_info(sym, tpe);
    let body = wildcard(l, tpe);
    let name = l.term_name("_");
    let bind_tpe = l.u.intern_type(Type::Sym(None, sym));
    let bind = l.tpd.node(Tree::Bind { name, body }, Span::NONE, Some(bind_tpe), Some(sym));
    l.positioned(bind);
    let value_tpe = l.tpd.tpe(value);
    let case = Tree::CaseDef { pat: bind, guard: None, body: value };
    let case = l.tpd.node(case, Span::NONE, Some(value_tpe), None);
    l.positioned(case)
}
