// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//! A pattern type of a class unrelated to the selector's constrained by the base types they
//! share (dotty's `constrainUpcasted` on a class application).

use super::super::Typer;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Whether pattern type `pat`, of a class neither deriving from nor derived by `sel`'s but
    /// possibly sharing an instance with it, conforms to `sel`'s base types at the parents of its
    /// class that `pat`'s class derives from, constraining type variables.
    pub fn constrain_upcast(&mut self, pat: TypeId, sel: TypeId) -> bool {
        let widened = self.u.widen(pat);
        let (Some(p), Some(s)) = (self.u.class_symbol(widened), self.u.class_symbol(sel)) else {
            return false;
        };
        if !self.u.is_class(p) || !self.u.is_class(s) || !matches!(self.u.types().get(sel), Type::Applied(..))
        {
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
}
