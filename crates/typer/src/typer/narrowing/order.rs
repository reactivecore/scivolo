// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ConstraintHandling.scala
//! The ordering between GADT-narrowed type parameters, kept on both sides (dotty's `addLess`).

use super::super::Typer;
use super::super::gadt::plain_type_ref;
use tasty::{SymbolId, Type, TypeId};

/// A narrowed symbol with its new lower and upper bound.
pub type Narrowed = (SymbolId, TypeId, TypeId);

/// The operands of `t`, an intersection if `upper`, else a union.
fn operands(u: &mut tasty::Universe, t: TypeId, upper: bool, out: &mut Vec<TypeId>) {
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

impl Typer<'_> {
    /// `narrowed` where each `s <: r` between symbols of `syms` that are not made one also bounds
    /// `r` from below by `s`, and each `r <: s` bounds `r` from above by `s`; the top operands of
    /// an intersection upper bound dropped.
    pub fn order_gadt(&mut self, narrowed: &mut Vec<Narrowed>, syms: &[SymbolId]) {
        let mut i = 0;
        while i < narrowed.len() {
            let (s, lo, hi) = narrowed[i];
            i += 1;
            if lo == hi {
                continue;
            }
            for upper in [true, false] {
                let mut parts = Vec::new();
                operands(self.u, if upper { hi } else { lo }, upper, &mut parts);
                let others: Vec<SymbolId> = parts
                    .iter()
                    .filter_map(|&p| plain_type_ref(self.u, p))
                    .filter(|r| *r != s && syms.contains(r))
                    .collect();
                if upper && parts.len() > 1 {
                    let kept: Vec<TypeId> = parts.into_iter().filter(|&p| !self.is_top_bound(p)).collect();
                    let hi = kept.into_iter().reduce(|a, b| self.u.intern_type(Type::And(a, b)));
                    narrowed[i - 1].2 = hi.unwrap_or(tasty::ANY);
                }
                for r in others {
                    self.add_gadt_bound(narrowed, r, s, !upper);
                }
            }
        }
    }

    /// Adds `s` to the upper bound of `r` if `upper`, else to its lower bound, in `narrowed`.
    fn add_gadt_bound(&mut self, narrowed: &mut Vec<Narrowed>, r: SymbolId, s: SymbolId, upper: bool) {
        let at = match narrowed.iter().position(|n| n.0 == r) {
            Some(at) => at,
            None => {
                let Some((lo, hi)) = self.current_bounds(r) else { return };
                narrowed.push((r, lo, hi));
                narrowed.len() - 1
            }
        };
        let (_, lo, hi) = narrowed[at];
        if lo == hi {
            return;
        }
        let own = self.gadt_ref(s);
        let mut parts = Vec::new();
        operands(self.u, if upper { hi } else { lo }, upper, &mut parts);
        if parts.iter().any(|&p| plain_type_ref(self.u, p) == Some(s)) {
            return;
        }
        if upper {
            narrowed[at].2 = if self.is_top_bound(hi) { own } else { self.u.intern_type(Type::And(hi, own)) };
        } else {
            narrowed[at].1 =
                if lo == tasty::NOTHING { own } else { self.u.intern_type(Type::Or(lo, own, false)) };
        }
    }

    /// Bounds `(lo, hi)` of pattern-bound type `s` with each of the narrowed types `params`
    /// bounded above by `s` added below, and each bounded below by `s` added above.
    pub fn ordered_bound_type(
        &mut self,
        s: SymbolId,
        (lo, hi): (TypeId, TypeId),
        params: &[SymbolId],
    ) -> (TypeId, TypeId) {
        let mut entry = vec![(s, lo, hi)];
        for &r in params {
            let Some((rlo, rhi)) = self.u.gadt_bounds(r) else { continue };
            for (bound, upper) in [(rhi, true), (rlo, false)] {
                let mut parts = Vec::new();
                operands(self.u, bound, upper, &mut parts);
                if parts.iter().any(|&p| plain_type_ref(self.u, p) == Some(s)) {
                    self.add_gadt_bound(&mut entry, s, r, !upper);
                }
            }
        }
        (entry[0].1, entry[0].2)
    }

    /// Whether `t` is `Any` or a type lambda of result `Any`.
    fn is_top_bound(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::Lambda { result, .. } => result == tasty::ANY,
            _ => t == tasty::ANY,
        }
    }
}
