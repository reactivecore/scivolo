//! Ranking successful implicit candidates (dotty's `compare` with `preferGeneral`): by the
//! derivation of their owners and by their types, first without their implicit clauses, then
//! with them; the same member by its prefix.

use super::super::Typer;
use super::candidates::Candidate;
use crate::ctx::Ctx;
use syntax::config::SourceVersion;
use tasty::tags::{IMPLICIT, MUTABLE};
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// The candidate of `found` that ranks above all others in `ctx`.
    pub fn most_specific(&mut self, found: &[Candidate], ctx: Ctx) -> Option<Candidate> {
        let mut alts: Vec<Candidate> = Vec::new();
        for &c in found {
            if !alts.contains(&c) {
                alts.push(c);
            }
        }
        if let [only] = alts[..] {
            return Some(only);
        }
        let v = self.source_version;
        let old_schemes: &[bool] = match v.is_at_least(SourceVersion::V3_7) {
            true if v != SourceVersion::V3_7.migrating_variant() => &[false],
            _ if v.is_at_least(SourceVersion::V3_6) => &[false, true],
            _ => &[true],
        };
        alts.iter().copied().find(|&c| {
            alts.iter()
                .filter(|&&d| d != c)
                .all(|&d| old_schemes.iter().all(|&old| self.compare(c, d, ctx, old) == Some(1)))
        })
    }

    /// 1 if `a` ranks above `b`, -1 if below, 0 if neither, by the rules before Scala 3.6 if
    /// `old`; `None` if undecided.
    fn compare(&mut self, a: Candidate, b: Candidate, ctx: Ctx, old: bool) -> Option<i32> {
        let (full_a, full_b) = (self.alt_type(a), self.alt_type(b));
        let (sa, sb) = (self.without_implicits(full_a)?, self.without_implicits(full_b)?);
        let result = self.compare_with_types((a, sa), (b, sb), ctx, old)?;
        Some(match (result, sa == full_a, sb == full_b) {
            (0, true, true) => 0,
            (0, true, false) => 1,
            (0, false, true) => -1,
            (0, false, false) => self.compare_with_types((a, full_a), (b, full_b), ctx, old)?,
            _ => result,
        })
    }

    fn compare_with_types(
        &mut self,
        (a, ta): (Candidate, TypeId),
        (b, tb): (Candidate, TypeId),
        ctx: Ctx,
        old: bool,
    ) -> Option<i32> {
        let owner = self.compare_owner(self.u.symbol(a.sym).owner, self.u.symbol(b.sym).owner);
        let wins_a = self.is_as_good((a, ta), (b, tb), ctx, old)?;
        let wins_b = self.is_as_good((b, tb), (a, ta), ctx, old)?;
        let same = |t: &mut Self| {
            let (ra, rb) = (t.ref_type(a.prefix, a.sym), t.ref_type(b.prefix, b.sym));
            a.sym == b.sym && t.u.is_subtype(ra, rb) && t.u.is_subtype(rb, ra)
        };
        if wins_a
            && wins_b
            && self.is_stable_alt(a)
            && (self.is_singleton(ta) || self.is_singleton(tb) || same(self))
        {
            return None;
        }
        let implicit = |t: &Self, c: Candidate| t.u.symbol(c.sym).flags.has(IMPLICIT);
        let new_scheme = !(old || implicit(self, a) && implicit(self, b));
        Some(match owner {
            1 if wins_a || !wins_b || new_scheme => 1,
            -1 if wins_b || !wins_a || new_scheme => -1,
            1 | -1 => 0,
            _ if wins_a != wins_b => i32::from(wins_a) - i32::from(wins_b),
            _ if a.sym == b.sym => self.compare_prefixes(a, b),
            _ => 0,
        })
    }

    /// `t` without the implicit clauses that lead it or its polymorphic result (dotty's
    /// `stripImplicit`); `None` for a result depending on a stripped parameter.
    fn without_implicits(&mut self, t: TypeId) -> Option<TypeId> {
        match self.u.types().get(t).clone() {
            Type::Method { binder, result, implicit: true, .. } => {
                let mut dependent = false;
                self.u.map_type(result, &mut |u, p| {
                    dependent |= matches!(u.types().get(p), Type::Param(b, _) if *b == binder);
                    None
                });
                if dependent { None } else { self.without_implicits(result) }
            }
            Type::Poly { binder, params, result } => {
                let stripped = self.without_implicits(result)?;
                match self.u.types().get(stripped) {
                    _ if stripped == result => Some(t),
                    Type::Poly { .. } => None,
                    _ => Some(self.u.intern_type(Type::Poly { binder, params, result: stripped })),
                }
            }
            _ => Some(t),
        }
    }

    fn is_stable_alt(&self, c: Candidate) -> bool {
        let s = self.u.symbol(c.sym);
        s.kind == Kind::Val && !s.flags.has(MUTABLE)
    }

    /// 1 if owner `x` derives from owner `y`, -1 the other way round, an object counting as its
    /// companion class when the larger of the two objects inherits no implicits; else 0 (dotty's
    /// `compareOwner`).
    fn compare_owner(&mut self, x: Option<SymbolId>, y: Option<SymbolId>) -> i32 {
        let (Some(x), Some(y)) = (x, y) else { return 0 };
        if x == y {
            return 0;
        }
        if self.u.is_class(x) && self.u.is_class(y) {
            if self.u.derives_from(x, y) {
                return 1;
            }
            if self.u.derives_from(y, x) {
                return -1;
            }
        }
        match (self.u.is_module_class(x), self.u.is_module_class(y)) {
            (true, true) => {
                let (cx, cy) = (self.u.companion_class(x), self.u.companion_class(y));
                let r = self.compare_owner(cx, cy);
                let larger = if r < 0 { x } else { y };
                let own = self
                    .implicit_members(larger, true)
                    .iter()
                    .all(|&m| self.u.symbol(m).owner == Some(larger));
                if own { r } else { 0 }
            }
            (true, false) => {
                let cx = self.u.companion_class(x);
                self.compare_owner(cx, Some(y))
            }
            (false, true) => {
                let cy = self.u.companion_class(y);
                self.compare_owner(Some(x), cy)
            }
            _ => 0,
        }
    }

    /// For two references to one member: 1 if `a`'s prefix is the more specific, -1 if `b`'s,
    /// an object's prefix counting as its parents (dotty's `comparePrefixes`).
    fn compare_prefixes(&mut self, a: Candidate, b: Candidate) -> i32 {
        let (pa, pb) = (self.widened_prefix(a.prefix), self.widened_prefix(b.prefix));
        let mark = self.constraint.mark();
        let wins_a = self.relaxed_conforms(pa, pb);
        self.constraint.rollback(mark);
        let wins_b = self.relaxed_conforms(pb, pa);
        self.constraint.rollback(mark);
        i32::from(wins_a) - i32::from(wins_b)
    }

    fn widened_prefix(&mut self, prefix: Option<TypeId>) -> TypeId {
        let Some(p) = prefix else { return self.u.intern_type(Type::NoPrefix) };
        let w = self.u.widen(p);
        match self.u.class_symbol(w) {
            Some(c) if self.u.is_module_class(c) && self.u.symbol(c).kind != Kind::Package => {
                self.parents_of(w, c)
            }
            _ => w,
        }
    }
}
