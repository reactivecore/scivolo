//! Each retained inline method given the body of its body retainer, which is dropped; the method
//! is then no longer inline (dotty's `Erasure.addRetainedInlineBodies`, run before the phases).

use super::lifted::substitute;
use super::walk::subtrees;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use tasty::tags::{INLINE, TRANSPARENT};
use typer::tpd::{TNodeId, Tree};

/// The suffix of the name of a body retainer (dotty's `BodyRetainerName`).
const RETAINED_BODY: &str = "$retainedBody";

pub fn transform(l: &mut Lower, root: TNodeId) -> Result<()> {
    for t in subtrees(l.tpd, root) {
        let Tree::Template { body, .. } = *l.tpd.tree(t) else { continue };
        let stats = l.tpd.items(body).to_vec();
        let mut retainers = Vec::new();
        for &s in &stats {
            if let Some(method) = retained(l, &stats, s) {
                let Some(method) = method else {
                    return unsupported(l.tpd.span(s), "overloaded retained inline methods");
                };
                retainers.push((s, method));
            }
        }
        if retainers.is_empty() {
            continue;
        }
        for &(r, method) in &retainers {
            move_body(l, r, method)?;
        }
        let retainers: Vec<TNodeId> = retainers.into_iter().map(|(r, _)| r).collect();
        let kept: Vec<TNodeId> = stats.into_iter().filter(|s| !retainers.contains(s)).collect();
        let kept = l.tpd.list(&kept);
        if let Tree::Template { body, .. } = &mut l.tpd.get_mut(t).tree {
            *body = kept;
        }
    }
    Ok(())
}

/// If `stat` is a body retainer, the definition among `stats` of its inline method: the only one
/// of its name, else the one of its type, `None` if neither.
fn retained(l: &mut Lower, stats: &[TNodeId], stat: TNodeId) -> Option<Option<TNodeId>> {
    let Tree::DefDef { .. } = l.tpd.tree(stat) else { return None };
    let sym = l.tpd.sym(stat)?;
    let name = l.u.symbol(sym).name.strip_suffix(RETAINED_BODY)?;
    let candidates: Vec<TNodeId> = stats
        .iter()
        .copied()
        .filter(|&s| {
            matches!(l.tpd.tree(s), Tree::DefDef { rhs: Some(_), .. })
                && l.tpd.sym(s).is_some_and(|m| l.u.symbol(m).name == name && l.u.symbol(m).flags.has(INLINE))
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let info = l.u.info(sym);
    let same_info = candidates.iter().copied().find(|&s| l.tpd.sym(s).is_some_and(|m| l.u.info(m) == info));
    Some(same_info.or((candidates.len() == 1).then(|| candidates[0])))
}

/// Gives method definition `method` the body of retainer `retainer`, its parameters and owner
/// replaced by the method's, and makes the method no longer inline.
fn move_body(l: &mut Lower, retainer: TNodeId, method: TNodeId) -> Result<()> {
    let (Some(from), Some(to)) = (l.tpd.sym(retainer), l.tpd.sym(method)) else { unreachable!("symbols") };
    let (from_params, to_params) = (params(l, retainer), params(l, method));
    if from_params.len() != to_params.len() {
        return unsupported(l.tpd.span(method), "retained inline method unlike its body retainer");
    }
    let Tree::DefDef { rhs: Some(body), .. } = *l.tpd.tree(retainer) else { unreachable!("a body") };
    let map: FxHashMap<_, _> = from_params.into_iter().zip(to_params).collect();
    substitute(l, body, &map);
    for t in subtrees(l.tpd, body) {
        let definition = matches!(
            l.tpd.tree(t),
            Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Bind { .. }
        );
        if definition && let Some(s) = l.tpd.sym(t).filter(|&s| l.u.symbol(s).owner == Some(from)) {
            l.u.set_owner(s, to);
        }
    }
    if let Tree::DefDef { rhs, .. } = &mut l.tpd.get_mut(method).tree {
        *rhs = Some(body);
    }
    let flags = l.u.symbol(to).flags.without(INLINE).without(TRANSPARENT);
    l.u.set_flags(to, flags);
    l.u.unenter(from);
    Ok(())
}

/// The value parameters of method definition `def`.
fn params(l: &Lower, def: TNodeId) -> Vec<tasty::SymbolId> {
    let Tree::DefDef { paramss, .. } = *l.tpd.tree(def) else { return Vec::new() };
    let clauses = l.tpd.clause_items(paramss).to_vec();
    clauses.iter().flat_map(|&c| l.tpd.items(c).to_vec()).filter_map(|p| l.tpd.sym(p)).collect()
}
