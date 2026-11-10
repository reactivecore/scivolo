// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NameKinds.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Retained inline methods, which override a method that is not inline and so keep a body at run
//! time: each followed by its body retainer (dotty's `isRetainedInline`, `Inlines.bodyRetainer`).

use super::super::Typer;
use crate::ctx::Ctx;
use crate::inlining::Unsupported;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::tags::INLINE;
use tasty::{Kind, SymbolId};

/// The suffix of the name of a body retainer (dotty's `BodyRetainerName`).
pub const RETAINED_BODY: &str = "$retainedBody";

/// Class body `body`, typed in `ctx`, with the body retainer of each retained inline method
/// after the method (dotty's `inlineExpansion`).
pub fn with_retainers(t: &mut Typer<'_>, body: Vec<TNodeId>, ctx: Ctx) -> Vec<TNodeId> {
    let mut out = Vec::with_capacity(body.len());
    for stat in body {
        out.push(stat);
        let Some(sym) = t.tpd.sym(stat).filter(|_| matches!(t.tpd.tree(stat), T::DefDef { .. })) else {
            continue;
        };
        if is_retained_inline(t, sym) {
            out.extend(body_retainer(t, stat, sym, ctx));
        }
    }
    out
}

/// Whether `sym` is a concrete inline method that overrides a method that is not inline.
fn is_retained_inline(t: &mut Typer<'_>, sym: SymbolId) -> bool {
    let s = t.u.symbol(sym);
    if s.kind != Kind::Def || !s.flags.has(INLINE) || t.u.is_deferred(sym) {
        return false;
    }
    let overridden = t.overridden(sym);
    overridden.into_iter().any(|m| !t.u.symbol(m).flags.has(INLINE))
}

/// `private def f$retainedBody(ps) = f(ps)` for retained inline method `f` defined by `def`,
/// entered and typed in `ctx`, the call to expand when the unit is inlined; `None`, reported,
/// for a method with type or implicit parameters.
fn body_retainer(t: &mut Typer<'_>, def: TNodeId, f: SymbolId, ctx: Ctx) -> Option<TNodeId> {
    let T::DefDef { paramss, tpt, rhs: Some(rhs), .. } = *t.tpd.tree(def) else { return None };
    let body = match *t.tpd.tree(rhs) {
        T::Typed { expr, .. } if t.tpd.span(rhs).is_synthetic() => expr,
        _ => rhs,
    };
    let span = t.tpd.span(body).start_pos();
    let flags = t.syms.flags.get(&f).copied().unwrap_or_default();
    let clauses: Vec<Vec<TNodeId>> =
        t.tpd.clause_items(paramss).iter().map(|&c| t.tpd.items(c).to_vec()).collect();
    let params: Vec<TNodeId> = clauses.iter().flatten().copied().collect();
    if params.iter().any(|&p| !matches!(t.tpd.tree(p), T::ValDef { .. })) {
        t.unsupported_at(t.tpd.span(def), "retained inline method with type parameters");
        return None;
    }
    let implicit = params
        .iter()
        .filter_map(|&p| t.tpd.sym(p))
        .any(|p| t.syms.flags.get(&p).is_some_and(|fl| fl.is_one_of(f::GIVEN_OR_IMPLICIT)));
    if implicit || flags.is(f::GIVEN_OR_IMPLICIT) {
        t.unsupported_at(t.tpd.span(def), "retained inline method with implicit parameters");
        return None;
    }

    let fun = t.member_ref(f, span);
    let mut call = t.ast.node(Tree::TypeTree);
    t.splices.insert(call, fun);
    let mut retainer_clauses = Vec::new();
    for clause in &clauses {
        let mut defs = Vec::new();
        let mut args = Vec::new();
        for &p in clause {
            let p = t.tpd.sym(p).expect("a parameter");
            let name = term_name_of(t, p);
            let info = t.u.info(p);
            let tpt = t.splice_type(info);
            let mods = t.ast.mods(Modifiers::with_flags(f::TERM_PARAM));
            defs.push(t.ast.node(Tree::ValDef { name, tpt, rhs: None, mods }));
            args.push(t.ast.node(Tree::Ident { name }));
        }
        let args = t.ast.list(&args);
        call = t.ast.node(Tree::Apply { fun: call, args });
        retainer_clauses.push(t.ast.list(&defs));
    }
    let result = t.tpd.tpe(tpt);
    let tpt = t.splice_type(result);
    let name = format!("{}{RETAINED_BODY}", t.u.symbol(f).name);
    let name = TName { name: t.name(&name), is_type: false };
    let paramss = t.ast.clauses(&retainer_clauses);
    let dropped = f::INLINE.or(f::MACRO).or(f::OVERRIDE).or(f::ABS_OVERRIDE).or(f::TRANSPARENT);
    let mods = t.ast.mods(Modifiers::with_flags(flags.and_not(dropped).or(f::PRIVATE)));
    let retainer: NodeId = t.ast.node(Tree::DefDef { name, paramss, tpt, rhs: Some(call), mods });
    t.ast.with_span(retainer, Span::at(span.start()));

    t.index(&[retainer], ctx);
    let sym = t.syms.by_tree[&retainer];
    let own = t.syms.flags.get(&sym).copied().unwrap_or_default().and_not(f::LOCAL);
    let own = if flags.is(f::TRANSPARENT) { own.or(f::TRANSPARENT) } else { own };
    t.syms.set_flags(t.u, sym, own);
    let typed = t.typed_stat(retainer, ctx);
    if let T::DefDef { rhs: Some(call), .. } = *t.tpd.tree(typed)
        && let Some(&call_ctx) = t.inline_ctxs.get(&call)
    {
        match expanded_retained(t, call, f, call_ctx) {
            Ok(inlined) => {
                let T::DefDef { name, paramss, tpt, .. } = *t.tpd.tree(typed) else { unreachable!() };
                t.tpd.get_mut(typed).tree = T::DefDef { name, paramss, tpt, rhs: Some(inlined) };
            }
            Err((at, what)) => t.unsupported_at(at, what),
        }
    }
    Some(typed)
}

/// Call `call` in `ctx` of retained inline method `f` expanded, as the typer expands
/// transparent calls.
fn expanded_retained(
    t: &mut Typer<'_>,
    call: TNodeId,
    f: SymbolId,
    ctx: Ctx,
) -> Result<TNodeId, Unsupported> {
    let span = t.tpd.span(call);
    t.define_call_types(call).map_err(|w| (span, w))?;
    let mut bodies = FxHashMap::default();
    t.collect_transparent_bodies(f, false, &mut bodies).map_err(|w| (span, w))?;
    let with_inits = t.classes_with_inits();
    t.with_inlining(true, |i| i.inline_transparent((bodies, with_inits), (call, true), ctx))
}

/// The term name of symbol `s`.
fn term_name_of(t: &mut Typer<'_>, s: SymbolId) -> TName {
    let text = t.u.symbol(s).name.clone();
    TName { name: t.name(&text), is_type: false }
}
