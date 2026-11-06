// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//! The members of an object class its static forwarders call, one per name and erased signature,
//! as dotty's `membersBasedOnFlags` finds them after erasure.

use crate::Lower;
use crate::symbols::has;
use rustc_hash::{FxHashMap, FxHashSet};
use tasty::tags::PRIVATE;
use tasty::{Erased, Kind, SymbolId};

type Signature = (String, (Vec<Erased>, Erased));

/// The members of `class` among those `keep` admits: a class's own members hide the inherited
/// ones of their signature (`computeMembersNamed`); of those inherited from different parents with
/// one signature, the one `mergeSingleDenot` picks: not a bridge, concrete, from the base first in
/// the linearization of `class`.
pub fn members(l: &mut Lower, class: SymbolId, keep: &dyn Fn(&mut Lower, SymbolId) -> bool) -> Vec<SymbolId> {
    let mut cache = FxHashMap::default();
    let all = collect(l, class, keep, &mut cache);
    let order = l.u.linearization(class);
    let mut chosen: Vec<(Signature, SymbolId)> = Vec::new();
    for m in all {
        let sig = signature(l, m);
        match chosen.iter().position(|(s, _)| *s == sig) {
            Some(i) if rank(l, &order, m) < rank(l, &order, chosen[i].1) => chosen[i].1 = m,
            Some(_) => {}
            None => chosen.push((sig, m)),
        }
    }
    chosen.into_iter().map(|(_, m)| m).collect()
}

/// The members of `c`: its own, then of each parent from the last one those not private, not
/// already found, whose signature none of its own has.
fn collect(
    l: &mut Lower,
    c: SymbolId,
    keep: &dyn Fn(&mut Lower, SymbolId) -> bool,
    cache: &mut FxHashMap<SymbolId, Vec<SymbolId>>,
) -> Vec<SymbolId> {
    if let Some(found) = cache.get(&c) {
        return found.clone();
    }
    let own: Vec<SymbolId> = l.u.decls(c).unwrap_or_default().into_iter().filter(|&m| keep(l, m)).collect();
    let own_sigs: Vec<Signature> = own.iter().map(|&m| signature(l, m)).collect();
    let parents: Vec<SymbolId> = l.u.parents(c).into_iter().filter_map(|p| l.u.class_symbol(p)).collect();
    let mut found = own;
    let mut inherited = FxHashSet::default();
    for p in parents.into_iter().rev() {
        for m in collect(l, p, keep, cache) {
            if has(l.u, m, PRIVATE) || inherited.contains(&m) || own_sigs.contains(&signature(l, m)) {
                continue;
            }
            inherited.insert(m);
            found.push(m);
        }
    }
    cache.insert(c, found.clone());
    found
}

fn signature(l: &mut Lower, m: SymbolId) -> Signature {
    (l.u.symbol(m).name.clone(), l.u.erased_method(m))
}

/// The rank of member `m` among members of one signature, lowest first: not a bridge, concrete,
/// its owner earliest in `order`.
fn rank(l: &mut Lower, order: &[SymbolId], m: SymbolId) -> (bool, bool, usize) {
    let owner = l.u.symbol(m).owner;
    let at = order.iter().position(|&b| Some(b) == owner).unwrap_or(order.len());
    (l.bridges.contains(&m), is_deferred(l, m), at)
}

/// Whether member `m` has no body; a parameter of a library class has one.
pub fn is_deferred(l: &mut Lower, m: SymbolId) -> bool {
    match l.u.symbol(m).tree.is_some() {
        true => l.u.symbol(m).kind != Kind::Param && l.u.body(m).is_none(),
        false => l.u.is_deferred(m),
    }
}
