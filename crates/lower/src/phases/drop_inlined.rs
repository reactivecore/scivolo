//! dotty's `Inlines.dropInlined`, run before the other phases: each `Inlined` tree becomes the
//! block of its bindings and expansion, or its expansion alone; its call is dropped. Under the
//! outermost one, the trees from another source take its span (`reposition`).

use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use typer::tpd::{TNodeId, Tree, map_children};

/// `t` with the `Inlined` trees in it dropped.
pub fn transform(l: &mut Lower, t: TNodeId) -> Result<TNodeId> {
    if let Some(at) = library_definition(l, t, None) {
        return unsupported(at, "definitions from library inline bodies");
    }
    Ok(drop(l, t, None))
}

/// The span of the inline call, inside the outermost one at `call` if any, whose expansion has
/// a definition from another source, which keeps its offsets in dotty.
fn library_definition(l: &Lower, t: TNodeId, call: Option<Span>) -> Option<Span> {
    let call =
        call.or_else(|| matches!(l.tpd.tree(t), Tree::Inlined { call: Some(_), .. }).then(|| l.tpd.span(t)));
    let def = matches!(l.tpd.tree(t), Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. });
    if def && l.tpd.span(t).is_in_other_source() {
        return Some(call.unwrap_or(Span::NONE));
    }
    l.tpd.children(t).into_iter().find_map(|c| library_definition(l, c, call))
}

/// `t` with the `Inlined` trees in it dropped, inside the outermost one at `call` if any.
fn drop(l: &mut Lower, t: TNodeId, call: Option<Span>) -> TNodeId {
    let call = call.or_else(|| match l.tpd.tree(t) {
        Tree::Inlined { call: Some(_), .. } => Some(l.tpd.span(t)),
        _ => None,
    });
    if let Some(at) = call {
        reposition(l, t, at);
    }
    if let Tree::Inlined { bindings, expansion, .. } = *l.tpd.tree(t) {
        if l.tpd.items(bindings).is_empty() {
            let expansion = drop(l, expansion, call);
            l.inlined_spans.insert(expansion, l.tpd.span(t));
            return expansion;
        }
        l.tpd.get_mut(t).tree = Tree::Block { stats: bindings, expr: expansion };
        l.inlined_blocks.insert(t);
    }
    map_children(l, t, &mut |l, c| drop(l, c, call));
    t
}

/// Gives `t`, if its span is in another source, the span `at` of the call, which the spans
/// derived from it until erasure keep.
fn reposition(l: &mut Lower, t: TNodeId, at: Span) {
    if l.tpd.span(t).is_in_other_source() {
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
