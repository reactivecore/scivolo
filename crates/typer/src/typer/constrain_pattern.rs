//! An extractor pattern's type constrained by a union selector type (dotty's
//! `constrainPatternType` on an `OrType` scrutinee, `necessaryEither`).

use super::Typer;
use tasty::{SymbolId, Type, TypeId, TypeVars, VarId};

impl Typer<'_> {
    /// Whether pattern type `pat` conforms to `sel`, constraining type variables; for a union
    /// `sel`, by each operand separately, keeping the weaker of two constraints or neither if
    /// they are incomparable.
    pub fn constrain_pattern(&mut self, pat: TypeId, sel: TypeId) -> Result<bool, &'static str> {
        let dealiased = self.dealiased(sel);
        let Type::Or(a, b, _) = *self.u.types().get(dealiased) else { return Ok(self.conforms(pat, sel)) };
        let mark = self.constraint.mark();
        if !self.constrain_operand(pat, a)? {
            self.constraint.rollback(mark);
            return self.constrain_operand(pat, b);
        }
        let left = self.constraint.changes_since(mark);
        let left_bounds = self.var_bounds(&left.vars);
        self.constraint.rollback(mark);
        if !self.constrain_operand(pat, b)? {
            self.constraint.rollback(mark);
            self.constraint.redo(left);
            return Ok(true);
        }
        let right = self.constraint.changes_since(mark);
        let mut vars = left.vars.clone();
        vars.extend(right.vars.iter().filter(|v| !left.vars.contains(v)));
        let right_bounds = self.var_bounds(&vars);
        self.constraint.rollback(mark);
        let pre_bounds = self.var_bounds(&vars);
        let left_bounds: Vec<(TypeId, TypeId)> = vars
            .iter()
            .zip(&pre_bounds)
            .map(|(v, &pre)| left.vars.iter().position(|w| w == v).map_or(pre, |i| left_bounds[i]))
            .collect();
        if self.subsumes(&left_bounds, &right_bounds, &pre_bounds) {
            self.constraint.redo(right);
        } else if self.subsumes(&right_bounds, &left_bounds, &pre_bounds) {
            self.constraint.redo(left);
        }
        Ok(true)
    }

    /// `constrain_pattern` for an operand of a union; outside the subset where it fails although
    /// the classes of `pat` and `sel` might share an instance.
    fn constrain_operand(&mut self, pat: TypeId, sel: TypeId) -> Result<bool, &'static str> {
        if self.constrain_pattern(pat, sel)? {
            return Ok(true);
        }
        let (p, s) = (self.u.class_symbol(pat), self.u.class_symbol(sel));
        let (Some(p), Some(s)) = (p, s) else { return Err("pattern against an operand of a union") };
        if self.classes_may_be_compatible(p, s) {
            return Err("pattern against an operand of a union");
        }
        Ok(false)
    }

    /// dotty's `classesMayBeCompatible`.
    fn classes_may_be_compatible(&mut self, p: SymbolId, s: SymbolId) -> bool {
        let flags = |t: &mut Self, c: SymbolId| t.u.symbol(c).flags;
        let (pf, sf) = (flags(self, p), flags(self, s));
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

    /// The bounds of `vars`, both an instance if a variable has one.
    fn var_bounds(&self, vars: &[VarId]) -> Vec<(TypeId, TypeId)> {
        vars.iter()
            .map(|&v| self.constraint.instance(v).map_or_else(|| self.constraint.bounds(v), |t| (t, t)))
            .collect()
    }

    /// Whether the bounds `c1` lie within the bounds `c2`: always if `c2` is `pre`, never if only
    /// `c1` is (dotty's `subsumes`).
    fn subsumes(
        &mut self,
        c1: &[(TypeId, TypeId)],
        c2: &[(TypeId, TypeId)],
        pre: &[(TypeId, TypeId)],
    ) -> bool {
        if c2 == pre {
            return true;
        }
        if c1 == pre {
            return false;
        }
        c1.iter()
            .zip(c2)
            .all(|(&(lo1, hi1), &(lo2, hi2))| self.u.is_subtype(lo2, lo1) && self.u.is_subtype(hi1, hi2))
    }
}
