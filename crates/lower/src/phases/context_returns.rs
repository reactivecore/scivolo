// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ContextFunctionResults.scala
//! Returns from a method whose context closures `ContextFunctionResults` integrated: each returns
//! the body of the closures its value is, over the method's parameters.

use super::walk::subtrees;
use crate::Lower;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

/// Makes each return from `m` in `body` whose value is `params.len()` nested closures return
/// their body, the closures' parameters replaced by `params`, one clause each.
pub fn integrate_returns(l: &mut Lower, body: TNodeId, m: SymbolId, params: &[Vec<SymbolId>]) {
    for t in subtrees(l.tpd, body) {
        let Tree::Return { expr, from } = *l.tpd.tree(t) else { continue };
        if l.tpd.sym(from) != Some(m) {
            continue;
        }
        let to = params.concat();
        let Some((anons, own, inner)) = closures(l, expr, params.len()).filter(|c| c.1.len() == to.len())
        else {
            continue;
        };
        for s in subtrees(l.tpd, inner).into_iter().filter_map(|s| l.tpd.sym(s)) {
            if l.u.symbol(s).owner.is_some_and(|o| anons.contains(&o)) {
                l.u.set_owner(s, m);
            }
        }
        super::local_classes::redirect(l, inner, &own, &to, None);
        if let Tree::Return { expr, .. } = &mut l.tpd.get_mut(t).tree {
            *expr = inner;
        }
    }
}

/// The methods and parameters of the `depth` closures nested in `t`, and the innermost body.
fn closures(l: &Lower, mut t: TNodeId, depth: usize) -> Option<(Vec<SymbolId>, Vec<SymbolId>, TNodeId)> {
    let (mut anons, mut params) = (Vec::new(), Vec::new());
    for _ in 0..depth {
        let anon = super::context_results::closure_def(l, t)?;
        let Tree::DefDef { paramss, rhs: Some(rhs), .. } = *l.tpd.tree(anon) else { return None };
        anons.push(l.tpd.sym(anon)?);
        for &c in l.tpd.clause_items(paramss) {
            params.extend(l.tpd.items(c).iter().filter_map(|&p| l.tpd.sym(p)));
        }
        t = rhs;
    }
    Some((anons, params, t))
}
