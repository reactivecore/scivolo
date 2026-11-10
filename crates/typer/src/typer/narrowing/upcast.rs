// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//! A pattern type of a class unrelated to the selector's constrained by the base types they
//! share (dotty's `constrainUpcasted` on a class application).

use super::super::type_ops::TypeOps;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl TypeOps<'_> {
    /// Tells whether pattern type `pat`, of a class neither deriving from nor derived by `sel`'s
    /// but possibly sharing an instance with it, conforms to `sel`'s base types at the parents of
    /// its class that `pat`'s class derives from, constraining type variables: `case c: C[a]`
    /// against `B[Int]` where both extend `A[X]`.
    pub fn constrain_upcast(&mut self, pat: TypeId, sel: TypeId) -> bool {
        let widened = self.u.widen(pat);
        let (Some(p), Some(s)) = (self.u.class_symbol(widened), self.u.class_symbol(sel)) else {
            return false;
        };
        if !self.u.is_class(p) || !self.u.is_class(s) || self.u.applied(sel).is_none() {
            return false;
        }
        if self.u.derives_from(p, s) || self.u.derives_from(s, p) || !self.classes_may_be_compatible(p, s) {
            return false;
        }
        let mut shared = Vec::new();
        self.shared_parents(s, p, &mut shared);
        let mut met = None;
        for c in shared {
            let Some(base) = self.u.base_type(sel, c) else { return false };
            met = Some(met.map_or(base, |m| self.u.intern_type(Type::And(m, base))));
        }
        let Some(met) = met else { return false };
        let mark = self.constraint.mark();
        let ok = self.conforms(pat, met);
        if !ok {
            self.constraint.rollback(mark);
        }
        ok
    }

    /// Adds the nearest base classes of class `c` that class `p` derives from to `out`, `Object`
    /// as a first parent left out.
    fn shared_parents(&mut self, c: SymbolId, p: SymbolId, out: &mut Vec<SymbolId>) {
        let parents = self.u.parents(c);
        for (i, t) in parents.into_iter().enumerate() {
            let Some(k) = self.u.class_symbol(t) else { continue };
            if i == 0 && self.u.full_name(k) == "java.lang.Object" {
                continue;
            }
            if self.u.derives_from(p, k) {
                if !out.contains(&k) {
                    out.push(k);
                }
            } else {
                self.shared_parents(k, p, out);
            }
        }
    }

    /// Tells whether classes `p` and `s` may have a common instance: a final one derives from the
    /// other, two classes one from the other; a trait always may (dotty's
    /// `classesMayBeCompatible`).
    pub fn classes_may_be_compatible(&mut self, p: SymbolId, s: SymbolId) -> bool {
        let (pf, sf) = (self.u.symbol(p).flags, self.u.symbol(s).flags);
        if pf.has(tasty::tags::FINAL) {
            self.u.derives_from(p, s)
        } else if sf.has(tasty::tags::FINAL) {
            self.u.derives_from(s, p)
        } else if !pf.has(tasty::tags::TRAIT) && !sf.has(tasty::tags::TRAIT) {
            self.u.derives_from(p, s) || self.u.derives_from(s, p)
        } else {
            true
        }
    }
}
