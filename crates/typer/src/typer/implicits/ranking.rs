//! Ranking successful implicit candidates (dotty's `compareAlternatives`): by the derivation of
//! their owners, then by `isAsSpecific` on their types.

use super::super::Typer;
use super::candidates::Candidate;
use tasty::{Type, TypeId, TypeVars};

impl Typer<'_> {
    /// The candidate of `found` that ranks above all others.
    pub fn most_specific(&mut self, found: &[Candidate]) -> Option<Candidate> {
        found
            .iter()
            .copied()
            .find(|&c| found.iter().filter(|&&d| d != c).all(|&d| self.compare_candidates(c, d) > 0))
    }

    /// 1 if `a` ranks above `b`, -1 if below, 0 if neither (dotty's `compare`).
    fn compare_candidates(&mut self, a: Candidate, b: Candidate) -> i32 {
        let (oa, ob) = (self.u.symbol(a.sym).owner, self.u.symbol(b.sym).owner);
        let owner_score = match (oa, ob) {
            (Some(x), Some(y)) if x != y && self.u.derives_from(x, y) => 1,
            (Some(x), Some(y)) if x != y && self.u.derives_from(y, x) => -1,
            _ => 0,
        };
        let wins_a = self.as_specific(a, b);
        let wins_b = self.as_specific(b, a);
        match owner_score {
            1 => i32::from(wins_a || !wins_b),
            -1 => -i32::from(wins_b || !wins_a),
            _ => i32::from(wins_a && !wins_b) - i32::from(wins_b && !wins_a),
        }
    }

    /// The parameter types of candidate `c`'s first explicit clause, or its value type, its type
    /// parameters left abstract or made fresh type variables.
    fn candidate_shape(&mut self, c: Candidate, fresh: bool) -> (Option<Vec<TypeId>>, TypeId) {
        let tpe = self.ref_type(c.prefix, c.sym);
        let mut info = self.method_info(tpe).unwrap_or(tpe);
        if let Type::Poly { binder, params, result } = self.u.types().get(info).clone() {
            info = result;
            if fresh {
                let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
                let vars: Vec<TypeId> = params.iter().map(|_| self.constraint.fresh(self.u)).collect();
                info = self.instantiate_poly(result, binder, &names, &vars, Some(c.sym));
            }
        }
        match self.u.types().get(info).clone() {
            Type::Method { params, result, implicit: false, .. } => {
                (Some(params.iter().map(|&(_, p)| p).collect()), result)
            }
            _ => (None, info),
        }
    }

    /// Whether candidate `a` is as specific as `b` (dotty's `isAsSpecific`): `b` applies to the
    /// parameters of `a`, or `a`'s value type conforms to `b`'s.
    fn as_specific(&mut self, a: Candidate, b: Candidate) -> bool {
        let mark = self.constraint.mark();
        let (pa, ta) = self.candidate_shape(a, false);
        let (pb, tb) = self.candidate_shape(b, true);
        let result = match (pa, pb) {
            (Some(pa), Some(pb)) => {
                pa.len() == pb.len() && pa.iter().zip(&pb).all(|(&x, &y)| self.conforms(x, y))
            }
            (None, None) => self.conforms(ta, tb),
            _ => false,
        };
        self.constraint.rollback(mark);
        result
    }
}
