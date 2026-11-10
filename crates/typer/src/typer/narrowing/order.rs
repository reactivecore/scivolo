// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! The ordering between GADT-narrowed type parameters, kept on both sides (dotty's `addLess`).

use super::super::gadt::{current_bounds, gadt_ref, plain_type_ref};
use super::super::gadt_full_bounds::is_top;
use tasty::{SymbolId, Type, TypeId, Universe};

/// A type parameter a pattern narrows, with its new bounds.
#[derive(Clone, Copy)]
pub struct Narrowed {
    pub sym: SymbolId,
    pub lo: TypeId,
    pub hi: TypeId,
}

/// The operands of `t`, an intersection if `upper`, else a union.
fn operands(u: &mut Universe, t: TypeId, upper: bool, out: &mut Vec<TypeId>) {
    match *u.types().get(t) {
        Type::And(a, b) if upper => {
            operands(u, a, upper, out);
            operands(u, b, upper, out);
        }
        Type::Or(a, b, _) if !upper => {
            operands(u, a, upper, out);
            operands(u, b, upper, out);
        }
        _ => out.push(t),
    }
}

/// Orders `narrowed`: each `S <: R` between parameters of `syms` that are not made one also
/// bounds `R` from below by `S`, and each `R <: S` bounds `R` from above by `S`, as for
/// `case p: Pair[a, b]` narrowing `A <: B`; the top operands of an intersection upper bound
/// dropped.
pub fn order(u: &mut Universe, narrowed: &mut Vec<Narrowed>, syms: &[SymbolId]) {
    let mut i = 0;
    while i < narrowed.len() {
        let Narrowed { sym: s, lo, hi } = narrowed[i];
        i += 1;
        if lo == hi {
            continue;
        }
        for upper in [true, false] {
            let mut parts = Vec::new();
            operands(u, if upper { hi } else { lo }, upper, &mut parts);
            let others: Vec<SymbolId> = parts
                .iter()
                .filter_map(|&p| plain_type_ref(u, p))
                .filter(|r| *r != s && syms.contains(r))
                .collect();
            if upper && parts.len() > 1 {
                let kept: Vec<TypeId> = parts.into_iter().filter(|&p| !is_top(u, p)).collect();
                let hi = kept.into_iter().reduce(|a, b| u.intern_type(Type::And(a, b)));
                narrowed[i - 1].hi = hi.unwrap_or(tasty::ANY);
            }
            for r in others {
                add_bound(u, narrowed, r, s, !upper);
            }
        }
    }
}

/// Adds `s` to the upper bound of `r` if `upper`, else to its lower bound, in `narrowed`.
fn add_bound(u: &mut Universe, narrowed: &mut Vec<Narrowed>, r: SymbolId, s: SymbolId, upper: bool) {
    let at = match narrowed.iter().position(|n| n.sym == r) {
        Some(at) => at,
        None => {
            let Some((lo, hi)) = current_bounds(u, r) else { return };
            narrowed.push(Narrowed { sym: r, lo, hi });
            narrowed.len() - 1
        }
    };
    let Narrowed { lo, hi, .. } = narrowed[at];
    if lo == hi {
        return;
    }
    let own = gadt_ref(u, s);
    let mut parts = Vec::new();
    operands(u, if upper { hi } else { lo }, upper, &mut parts);
    if parts.iter().any(|&p| plain_type_ref(u, p) == Some(s)) {
        return;
    }
    if upper {
        narrowed[at].hi = if is_top(u, hi) { own } else { u.intern_type(Type::And(hi, own)) };
    } else {
        narrowed[at].lo = if lo == tasty::NOTHING { own } else { u.intern_type(Type::Or(lo, own, false)) };
    }
}

/// Gives pattern-bound type `s` bounds `(lo, hi)` with each of the narrowed types `params`
/// bounded above by `s` added below, and each bounded below by `s` added above.
pub fn ordered_bound_type(
    u: &mut Universe,
    s: SymbolId,
    (lo, hi): (TypeId, TypeId),
    params: &[SymbolId],
) -> (TypeId, TypeId) {
    let mut entry = vec![Narrowed { sym: s, lo, hi }];
    for &r in params {
        let Some((rlo, rhi)) = u.gadt_bounds(r) else { continue };
        for (bound, upper) in [(rhi, true), (rlo, false)] {
            let mut parts = Vec::new();
            operands(u, bound, upper, &mut parts);
            if parts.iter().any(|&p| plain_type_ref(u, p) == Some(s)) {
                add_bound(u, &mut entry, s, r, !upper);
            }
        }
    }
    (entry[0].lo, entry[0].hi)
}
