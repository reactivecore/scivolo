// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala

use super::local_implicits::{implicit_defs, is_implicit_def};
use crate::tpd::{TNodeId, Tpd, Tree as T};
use tasty::{SymbolId, Universe};

/// Determines the scopes of the given and implicit terms `t` defines in whose scope `call` is,
/// outermost first: `t` if it defines one, then for each tree containing `call` the terms its
/// children define, or a case's pattern binds; `None` if `t` does not contain `call`.
pub fn implicit_scopes_at(
    tpd: &Tpd,
    u: &Universe,
    (t, call): (TNodeId, TNodeId),
) -> Option<Vec<Vec<SymbolId>>> {
    let mut scopes = Vec::new();
    if !scopes_within(tpd, u, (t, call), &mut scopes) {
        return None;
    }
    scopes.extend(tpd.sym(t).filter(|_| is_implicit_def(tpd, u, t)).map(|s| vec![s]));
    scopes.reverse();
    Some(scopes)
}

/// Adds the scopes of `implicit_scopes_at` inside `t` to `found`, innermost first; whether `t`
/// contains `call`.
fn scopes_within(
    tpd: &Tpd,
    u: &Universe,
    (t, call): (TNodeId, TNodeId),
    found: &mut Vec<Vec<SymbolId>>,
) -> bool {
    if t == call {
        return true;
    }
    let children = tpd.children(t);
    if !children.iter().any(|&c| scopes_within(tpd, u, (c, call), found)) {
        return false;
    }
    let mut defined = Vec::new();
    match *tpd.tree(t) {
        T::CaseDef { pat, .. } => implicit_defs(tpd, u, pat, &mut defined),
        _ => defined
            .extend(children.into_iter().filter(|&c| is_implicit_def(tpd, u, c)).filter_map(|c| tpd.sym(c))),
    }
    if !defined.is_empty() {
        found.push(defined);
    }
    true
}
