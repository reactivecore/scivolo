// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! dotty's `Inlines.dropInlined`, run before the other phases: each `Inlined` tree becomes the
//! block of its bindings and expansion, or its expansion alone; its call is dropped. Under one
//! outside the expansions of others, the trees from another source but method and class
//! definitions take its span (`reposition`).

use crate::{Lower, Result};
use syntax::ast::Span;
use syntax::source::Offset;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree, map_children};

/// `t` with the `Inlined` trees in it dropped.
pub fn transform(l: &mut Lower, t: TNodeId) -> Result<TNodeId> {
    Ok(drop(l, t, None, 0))
}

/// `t` with the `Inlined` trees in it dropped, its trees from another source at `call` if any;
/// `depth` the number of expansions `t` is in, less the arguments inlined from outside (`Inlined`
/// trees without a call) between them (dotty's `enclosingInlineds`).
fn drop(l: &mut Lower, t: TNodeId, call: Option<Span>, depth: u32) -> TNodeId {
    let (call, inner) = match l.tpd.tree(t) {
        Tree::Inlined { call: Some(_), .. } if depth == 0 => (Some(l.tpd.span(t)), 1),
        Tree::Inlined { call: Some(_), .. } => (call, depth + 1),
        Tree::Inlined { call: None, .. } => (call, depth.saturating_sub(1)),
        _ => (call, depth),
    };
    if let Some(at) = call {
        if !matches!(l.tpd.tree(t), Tree::DefDef { .. } | Tree::TypeDef { .. }) {
            reposition(l, t, at);
        }
        let def = matches!(l.tpd.tree(t), Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. });
        if let Some(s) = l.tpd.sym(t).filter(|_| def) {
            l.inlined_at.insert(s, at.start());
        }
    }
    if let Tree::Inlined { bindings, expansion, .. } = *l.tpd.tree(t) {
        let stats: Vec<TNodeId> = l.tpd.items(bindings).to_vec();
        let stats: Vec<TNodeId> = stats.into_iter().map(|b| drop(l, b, call, depth)).collect();
        let expansion = drop(l, expansion, call, inner);
        if stats.is_empty() {
            l.inlined_spans.insert(expansion, l.tpd.span(t));
            match l.own_spans.get(&t).copied() {
                Some(own) => l.own_spans.insert(expansion, own),
                None => l.own_spans.remove(&expansion),
            };
            return expansion;
        }
        let stats = l.tpd.list(&stats);
        l.tpd.get_mut(t).tree = Tree::Block { stats, expr: expansion };
        l.inlined_blocks.insert(t);
        return t;
    }
    map_children(l, t, &mut |l, c| drop(l, c, call, depth));
    t
}

/// Gives `t`, if its span is in another source than the call's, the span `at` of the call, which
/// the spans derived from it until erasure keep.
fn reposition(l: &mut Lower, t: TNodeId, at: Span) {
    let span = l.tpd.span(t);
    let file = |s: Span| l.sources.index(s.without_inlined_call().start());
    let other_file = span.exists() && !span.is_inlined_call() && file(span) != file(at);
    if span.is_in_other_source() {
        l.own_spans.insert(t, span);
    }
    if span.is_in_other_source() || other_file {
        l.tpd.get_mut(t).span = at.as_inlined_call();
    }
}

/// The span `t` had in another source before `drop_inlined` gave it the span of an inline call,
/// kept by the definitions later phases make of `t`; else its span before erasure.
pub fn own_span(l: &Lower, t: TNodeId) -> Span {
    l.own_spans.get(&t).copied().unwrap_or_else(|| span_before_erasure(l, t))
}

/// Makes the spans of inline calls given to trees from another source plain spans, as erasure
/// leaves them.
pub fn unmark_calls(l: &mut Lower) {
    for i in 0..l.tpd.len() {
        let node = l.tpd.get_mut(TNodeId::new(i));
        node.span = node.span.without_inlined_call();
    }
    for span in l.inlined_spans.values_mut() {
        *span = span.without_inlined_call();
    }
}

/// The span of `t` as phases before erasure see it: that of the outermost `Inlined` tree it was
/// the expansion of, if any.
pub fn span_before_erasure(l: &Lower, t: TNodeId) -> Span {
    l.inlined_spans.get(&t).copied().unwrap_or_else(|| l.tpd.span(t))
}

/// Gives the trees of `adapted` around `t` the span of the inline call `t` is an argument inlined
/// into from another source, if it is one: dotty adapts a qualifier in the context of the
/// expansion, and `dropInlined` repositions what it made there.
pub fn adaptation_at_call(l: &mut Lower, adapted: TNodeId, t: TNodeId) {
    let call = span_before_erasure(l, t);
    if !call.is_inlined_call() {
        return;
    }
    let mut pending = vec![adapted];
    while let Some(n) = pending.pop() {
        if n != t {
            l.tpd.get_mut(n).span = call;
            pending.extend(super::walk::children(l.tpd, n));
        }
    }
}

/// The key `LambdaLift` names definition `sym` starting at `at` by: the start of the inline call
/// it was inlined at, if any, then `at`, in the order of the trees.
pub fn naming_order(l: &Lower, sym: SymbolId, at: Offset) -> (Offset, Offset) {
    let tree_offset = |at| super::companion_order::tree_offset(l, at, Some(sym));
    (tree_offset(l.inlined_at.get(&sym).copied().unwrap_or(at)), tree_offset(at))
}

/// Whether `t` stands for an `Inlined` tree phases before erasure see.
pub fn is_inlined(l: &Lower, t: TNodeId) -> bool {
    l.inlined_spans.contains_key(&t) || l.inlined_blocks.contains(&t)
}
