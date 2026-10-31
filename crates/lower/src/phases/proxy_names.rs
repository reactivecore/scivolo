// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Dependencies.scala
//   compiler/src/dotty/tools/dotc/transform/LambdaLift.scala
//! The order in which dotty's `LambdaLift` names the proxies: by owner as `Dependencies` found
//! them, `CollectDependencies`' first, then `computeFreeVars`' with the `$adapted` methods'.

use super::dependencies::Dependencies;
use super::lift_names::{Pending, Proxy, fresh};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::SymbolId;

/// Names `proxies` in the order of the dependencies of `pending`: by owner, then by value (dotty's
/// `generateProxies`); those `computeFreeVars` finds wait for `name_late`.
pub fn name_proxies(l: &mut Lower, proxies: &[Proxy], pending: &mut Pending) -> Result<()> {
    let mut ranked = Vec::new();
    let trait_params = l.trait_methods.values().flat_map(|(_, pairs)| pairs);
    let class_params = pending.proxies.iter().map(|p| (p.sym, p.value));
    let originals: FxHashMap<SymbolId, SymbolId> =
        trait_params.map(|&(v, p)| (p, v)).chain(class_params).collect();
    for p in proxies {
        let value = originals.get(&p.value).copied().unwrap_or(p.value);
        let deps = pending.deps.as_ref();
        let Some(rank) = deps.and_then(|d| d.rank(p.owner, value)) else {
            return unsupported(Span::NONE, "lifting a value the dependencies of the unit do not record");
        };
        match deps.and_then(|d| late_at(d, p.owner)) {
            Some(at) => pending.late.push((rank, at, p.sym)),
            None => ranked.push((rank, p.sym)),
        }
    }
    ranked.sort_by_key(|&(rank, _)| rank);
    let syms: Vec<SymbolId> = ranked.into_iter().map(|(_, s)| s).collect();
    name_each(l, &syms);
    Ok(())
}

/// The index among the callers of `owner`, 0 if not one, if `computeFreeVars` found its values.
fn late_at(deps: &Dependencies, owner: SymbolId) -> Option<usize> {
    let i = deps.free.iter().position(|(o, _)| *o == owner)?;
    (i >= deps.traversed).then(|| deps.callers.iter().position(|&c| c == owner).unwrap_or(0))
}

/// Names the proxies `computeFreeVars` finds: those of `pending` and of the `$adapted` method of
/// each lambda of `adapted`, those before the first of `pending` whose owner was found as a
/// caller after the lambda's definition.
pub fn name_late(l: &mut Lower, pending: &mut Pending, adapted: &[Adapted]) {
    let mut late = std::mem::take(&mut pending.late);
    late.sort_by_key(|&(rank, _, _)| rank);
    for i in 1..late.len() {
        late[i].1 = late[i].1.max(late[i - 1].1);
    }
    let mut syms: Vec<SymbolId> = late.iter().map(|&(_, _, s)| s).collect();
    let mut groups: Vec<(usize, &[SymbolId])> = adapted
        .iter()
        .map(|(f, ps)| {
            (pending.deps.as_ref().and_then(|d| d.ends.get(f).copied()).unwrap_or(usize::MAX), &ps[..])
        })
        .collect();
    groups.sort_by_key(|&(end, _)| end);
    for (end, proxies) in groups.into_iter().rev() {
        let at = late.iter().position(|&(_, caller, _)| caller >= end).unwrap_or(late.len());
        syms.splice(at..at, proxies.iter().copied());
    }
    name_each(l, &syms);
}

/// A lambda and the proxies of its `$adapted` method.
pub type Adapted = (SymbolId, Vec<SymbolId>);

/// Names proxies `proxies`, each `x$N` for the value `x` it stands for.
pub fn name_each(l: &mut Lower, proxies: &[SymbolId]) {
    for &p in proxies {
        let base = l.u.symbol(p).name.clone();
        let name = fresh(l, &base);
        l.u.rename(p, &name);
    }
}
