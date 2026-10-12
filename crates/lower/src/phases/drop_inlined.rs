//! dotty's `Inlines.dropInlined`, run before the other phases: each `Inlined` tree becomes the
//! block of its bindings and expansion, or its expansion alone; its call is dropped.

use crate::Lower;
use syntax::ast::Span;
use typer::tpd::{TNodeId, Tree, map_children};

/// `t` with the `Inlined` trees in it dropped.
pub fn transform(l: &mut Lower, t: TNodeId) -> TNodeId {
    if let Tree::Inlined { bindings, expansion, .. } = *l.tpd.tree(t) {
        if l.tpd.items(bindings).is_empty() {
            let expansion = transform(l, expansion);
            l.inlined_spans.insert(expansion, l.tpd.span(t));
            return expansion;
        }
        l.tpd.get_mut(t).tree = Tree::Block { stats: bindings, expr: expansion };
        l.inlined_blocks.insert(t);
    }
    map_children(l, t, &mut |l, c| transform(l, c));
    t
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
