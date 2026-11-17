// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Whether typing kept a type tree as written (dotty reuses such untyped trees as typed ones).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Attrs, NodeId, Tree};

/// Determines whether typing applied type tree `tpt`, written as such, as `typed` kept its type
/// arguments as written, as `String` of `Option[String]` but not `?` of `List[?]`, `t` of
/// `List[t]` or `(String)` of `Option[(String)]` (dotty's `tpt.args == tpt2.args` in
/// `typedUnApply`, true where typing reused the untyped trees).
pub fn args_kept(t: &Typer<'_>, tpt: NodeId, typed: TNodeId) -> bool {
    if is_rewritten(t, tpt) {
        return false;
    }
    match (t.ast.tree(tpt), t.tpd.tree(typed)) {
        (Tree::AppliedTypeTree { args: l, .. }, T::AppliedTypeTree { args: m, .. }) => {
            items_kept(t, t.ast.items(*l), t.tpd.items(*m))
        }
        _ => false,
    }
}

/// Whether the parser or desugaring rewrote `tpt` from what dotty types: parentheses dropped or
/// an infix type made an applied one.
fn is_rewritten(t: &Typer<'_>, tpt: NodeId) -> bool {
    t.ast.parens_span(tpt).is_some() || t.ast.attrs(tpt).contains(Attrs::INFIX_OP)
}

/// Whether trees `l` were typed as `m`, each kept.
fn items_kept(t: &Typer<'_>, l: &[NodeId], m: &[TNodeId]) -> bool {
    l.len() == m.len() && l.iter().zip(m).all(|(&a, &b)| kept(t, a, b))
}

/// Whether typing kept tree `a` as written as `b`: names resolved without a prefix to insert,
/// selections and applications of kept trees, singleton types of kept references, and types
/// annotated with a kept annotation.
fn kept(t: &Typer<'_>, a: NodeId, b: TNodeId) -> bool {
    if is_rewritten(t, a) {
        return false;
    }
    match (t.ast.tree(a), t.tpd.tree(b)) {
        (Tree::Ident { name: x }, T::Ident { name: y }) => x == y,
        (Tree::Select { qualifier: a, name: x }, T::Select { qualifier: b, name: y }) => {
            x == y && kept(t, *a, *b)
        }
        (Tree::This { .. }, T::This { .. }) => true,
        (Tree::Literal { value: x }, T::Literal { value: y }) => x == y,
        (Tree::SingletonTypeTree { reference: a }, T::SingletonTypeTree { reference: b }) => kept(t, *a, *b),
        (Tree::AppliedTypeTree { tpt: a, args: l }, T::AppliedTypeTree { tpt: b, args: m }) => {
            kept(t, *a, *b) && items_kept(t, t.ast.items(*l), t.tpd.items(*m))
        }
        (Tree::Annotated { arg: a, annot: x }, T::Annotated { arg: b, annot: y }) => {
            kept(t, *a, *b) && annotation_kept(t, *x, *y)
        }
        _ => false,
    }
}

/// Whether typing kept annotation `a`, `@C(args)`, as `b`: its class and arguments kept, none
/// added; dotty rebuilds one given default arguments, as `@deprecated`, or inferred type
/// arguments, as `@C[T]` for `@C` of `class C[T]`.
fn annotation_kept(t: &Typer<'_>, a: NodeId, b: TNodeId) -> bool {
    let (Tree::Apply { fun: f, args: l }, T::Apply { fun: g, args: m }) = (t.ast.tree(a), t.tpd.tree(b))
    else {
        return false;
    };
    let (Tree::Select { qualifier: q, .. }, T::Select { qualifier: r, .. }) =
        (t.ast.tree(*f), t.tpd.tree(*g))
    else {
        return false;
    };
    let (Tree::New { tpt: c }, T::New { tpt: d }) = (t.ast.tree(*q), t.tpd.tree(*r)) else { return false };
    kept(t, *c, *d) && items_kept(t, t.ast.items(*l), t.tpd.items(*m))
}
