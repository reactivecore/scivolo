//! The inferred element type trees of the repeated arguments of a file whose parameters fix them
//! without capture sets.

use super::definitions::is_class_type;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use crate::typer::capture_types::has_captures;
use rustc_hash::FxHashSet;
use tasty::Universe;

/// Collects the inferred element type tree, and the repeated type tree of its ascription, of each
/// sequence literal passed in `root` to a repeated parameter of a class type without type arguments
/// or capture sets, of a method without type parameters called with one argument list: the
/// `String` of `StringContext("a", "b")`, which dotty takes from the parameter.
pub fn pure_elements(u: &mut Universe, tpd: &Tpd, root: TNodeId) -> FxHashSet<TNodeId> {
    let mut pure = FxHashSet::default();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        stack.extend(tpd.children(n));
        let T::Apply { fun, args } = *tpd.tree(n) else { continue };
        let (T::Ident { .. } | T::Select { .. }, Some(s)) = (tpd.tree(fun), tpd.sym(fun)) else { continue };
        let info = u.info(s);
        let Some(formal) = u.method(info).and_then(|m| m.param_types().last().copied()) else { continue };
        let Some(elem) = u.repeated_elem(formal) else { continue };
        if has_captures(u, elem) || u.applied(elem).is_some() || !is_class_type(u, elem) {
            continue;
        }
        let Some(&last) = tpd.items(args).last() else { continue };
        let (seq, ascription) = match *tpd.tree(last) {
            T::Typed { expr, tpt } => (expr, Some(tpt)),
            _ => (last, None),
        };
        if let T::SeqLiteral { elemtpt, .. } = *tpd.tree(seq) {
            pure.extend([elemtpt].into_iter().chain(ascription));
        }
    }
    pure
}
