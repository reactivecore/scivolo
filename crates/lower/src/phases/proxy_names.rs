// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LambdaLift.scala
//! The order in which dotty's `LambdaLift` names the proxies (`generateProxies`): by owner as
//! `Dependencies` found them, then by value.

use super::lift_names::{Pending, Proxy, fresh};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::SymbolId;

/// Names those of `proxies` whose owners `CollectDependencies` found; the others wait in
/// `pending` for `name_late`.
pub fn name_proxies(l: &mut Lower, proxies: &[Proxy], pending: &mut Pending) -> Result<()> {
    let mut ranked = Vec::new();
    let trait_params = l.trait_methods.values().flat_map(|(_, pairs)| pairs);
    let class_params = pending.proxies.iter().map(|p| (p.sym, p.value));
    let originals: FxHashMap<SymbolId, SymbolId> =
        trait_params.map(|&(v, p)| (p, v)).chain(class_params).collect();
    for p in proxies {
        let value = originals.get(&p.value).copied().unwrap_or(p.value);
        let deps = pending.deps.as_ref();
        let Some((rank, traversed)) = deps.and_then(|d| Some((d.rank(p.owner, value)?, d.traversed))) else {
            return unsupported(Span::NONE, "lifting a value the dependencies of the unit do not record");
        };
        match rank.0 < traversed {
            true => ranked.push((rank, p.sym)),
            false => pending.late.push((rank, p.sym)),
        }
    }
    name_ranked(l, ranked);
    Ok(())
}

/// Names the proxies of `pending` and of the `$adapted` method of each lambda of `adapted`.
pub fn name_late(l: &mut Lower, pending: &mut Pending, adapted: &[Adapted]) {
    let mut late = std::mem::take(&mut pending.late);
    for (f, proxies) in adapted {
        for (j, &p) in proxies.iter().enumerate() {
            let rank = pending.deps.as_ref().and_then(|d| d.adapted_rank(*f, j));
            late.push((rank.unwrap_or((usize::MAX, j)), p));
        }
    }
    name_ranked(l, late);
}

/// A lambda and the proxies of its `$adapted` method.
pub type Adapted = (SymbolId, Vec<SymbolId>);

fn name_ranked(l: &mut Lower, mut ranked: Vec<((usize, usize), SymbolId)>) {
    ranked.sort_by_key(|&(rank, _)| rank);
    for (_, p) in ranked {
        let base = l.u.symbol(p).name.clone();
        let name = fresh(l, &base);
        l.u.rename(p, &name);
    }
}
