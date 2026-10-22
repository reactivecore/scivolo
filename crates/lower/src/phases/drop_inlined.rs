// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inlines.scala
//! dotty's `Inlines.dropInlined`, run before the other phases: each `Inlined` tree becomes the
//! block of its bindings and expansion, or its expansion alone; its call is dropped. Under one
//! outside the expansions of others, the trees from another source take its span (`reposition`).

use crate::{Lower, Result};
use syntax::ast::Span;
use typer::tpd::{TNodeId, Tree, map_children};

/// `t` with the `Inlined` trees in it dropped.
pub fn transform(l: &mut Lower, t: TNodeId) -> Result<TNodeId> {
    Ok(drop(l, t, None, false))
}

/// `t` with the `Inlined` trees in it dropped, its trees from another source at `call` if any;
/// `expanded` if `t` is in the expansion of one.
fn drop(l: &mut Lower, t: TNodeId, call: Option<Span>, expanded: bool) -> TNodeId {
    let call = match l.tpd.tree(t) {
        Tree::Inlined { call: Some(_), .. } if !expanded => Some(l.tpd.span(t)),
        _ => call,
    };
    if let Some(at) = call {
        reposition(l, t, at);
    }
    if let Tree::Inlined { bindings, expansion, .. } = *l.tpd.tree(t) {
        let stats: Vec<TNodeId> = l.tpd.items(bindings).to_vec();
        let stats: Vec<TNodeId> = stats.into_iter().map(|b| drop(l, b, call, expanded)).collect();
        let expansion = drop(l, expansion, call, expanded || call.is_some());
        if stats.is_empty() {
            l.inlined_spans.insert(expansion, l.tpd.span(t));
            return expansion;
        }
        let stats = l.tpd.list(&stats);
        l.tpd.get_mut(t).tree = Tree::Block { stats, expr: expansion };
        l.inlined_blocks.insert(t);
        return t;
    }
    map_children(l, t, &mut |l, c| drop(l, c, call, expanded));
    t
}

/// Gives `t`, if its span is in another source than the call's, the span `at` of the call, which
/// the spans derived from it until erasure keep.
fn reposition(l: &mut Lower, t: TNodeId, at: Span) {
    let span = l.tpd.span(t);
    let file = |s: Span| l.sources.index(s.without_inlined_call().start());
    let other_file = span.exists() && !span.is_inlined_call() && file(span) != file(at);
    if span.is_in_other_source() || other_file {
        l.tpd.get_mut(t).span = at.as_inlined_call();
    }
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

/// Whether `t` stands for an `Inlined` tree phases before erasure see.
pub fn is_inlined(l: &Lower, t: TNodeId) -> bool {
    l.inlined_spans.contains_key(&t) || l.inlined_blocks.contains(&t)
}
