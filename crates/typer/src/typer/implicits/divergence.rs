// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Whether trying a candidate would make the implicit search diverge (dotty's
//! `checkDivergence`).

use super::super::Typer;
use super::candidates::Candidate;
use super::measures::Measure;
use tasty::{Binder, Type, TypeId};

impl Typer<'_> {
    /// Whether candidate `c` for `pt` diverges: an equivalent candidate is open on a type no
    /// smaller built from the same symbols, and no by-name argument lies between to tie a knot.
    pub fn diverges(&mut self, c: Candidate, pt: TypeId, by_name: bool) -> Result<bool, &'static str> {
        let mut below = by_name;
        let mut current: Option<Measure> = None;
        for i in (0..self.history.open.len()).rev() {
            let (cand, tp, tp_by_name, view) = {
                let o = &self.history.open[i];
                (o.cand, o.pt, o.by_name, o.view)
            };
            if self.equivalent(cand, c) {
                if view {
                    return Err("divergence check through a conversion");
                }
                let (wild_tp, wild_pt) = (self.wild_approx(tp), self.wild_approx(pt));
                if below && self.frozen_conforms(wild_tp, wild_pt) {
                    self.fully_defined_as(tp, true)?;
                    return Ok(false);
                }
                let prev = match self.history.open[i].measure.clone() {
                    Some(m) => m,
                    None => self.measure(tp)?,
                };
                self.history.open[i].measure = Some(prev.clone());
                let cur = match current.clone() {
                    Some(m) => m,
                    None => self.measure(pt)?,
                };
                current = Some(cur.clone());
                if prev.size <= cur.size
                    && prev.covering == cur.covering
                    && (prev.size < cur.size || self.frozen_same(wild_tp, wild_pt))
                {
                    return Ok(true);
                }
            }
            below |= tp_by_name;
        }
        Ok(false)
    }

    /// Whether `a <: b` without constraining type variables.
    fn frozen_conforms(&mut self, a: TypeId, b: TypeId) -> bool {
        let before = self.constraint.attempt();
        let result = self.conforms(a, b);
        self.constraint.abandon(before);
        result
    }

    fn frozen_same(&mut self, a: TypeId, b: TypeId) -> bool {
        a == b || self.frozen_conforms(a, b) && self.frozen_conforms(b, a)
    }

    /// Whether `a` and `b` are the same reference, or siblings of the same type (dotty's
    /// `equivalentCandidates`).
    fn equivalent(&mut self, a: Candidate, b: Candidate) -> bool {
        if a == b {
            return true;
        }
        if self.u.symbol(a.sym).owner != self.u.symbol(b.sym).owner {
            return false;
        }
        let (ta, tb) = (self.ref_type(a.prefix, a.sym), self.ref_type(b.prefix, b.sym));
        let ta = self.method_info(ta).unwrap_or(ta);
        let tb = self.method_info(tb).unwrap_or(tb);
        self.same_signature(ta, tb)
    }

    /// Whether method or value types `a` and `b` are the same, their parameters matched by
    /// position.
    fn same_signature(&mut self, a: TypeId, b: TypeId) -> bool {
        let (a, b) = (self.zonk(a), self.zonk(b));
        let sides = (self.u.types().get(a).clone(), self.u.types().get(b).clone());
        let ((ba, pa, ra), (bb, pb, rb)) = match sides {
            (
                Type::Poly { binder: ba, params: pa, result: ra },
                Type::Poly { binder: bb, params: pb, result: rb },
            ) => ((ba, pa, ra), (bb, pb, rb)),
            (
                Type::Method { binder: ba, params: pa, result: ra, implicit: ia, .. },
                Type::Method { binder: bb, params: pb, result: rb, implicit: ib, .. },
            ) if ia == ib => ((ba, pa, ra), (bb, pb, rb)),
            (Type::Poly { .. } | Type::Method { .. }, _) | (_, Type::Poly { .. } | Type::Method { .. }) => {
                return false;
            }
            _ => return self.frozen_same(a, b),
        };
        if pa.len() != pb.len() {
            return false;
        }
        let renamed = |t: &mut Self, x: TypeId| rebound(t, x, bb, ba);
        for ((_, x), (_, y)) in pa.into_iter().zip(pb) {
            let y = renamed(self, y);
            if !self.frozen_same(x, y) {
                return false;
            }
        }
        let rb = renamed(self, rb);
        self.same_signature(ra, rb)
    }
}

/// `t` with the parameters of binder `from` referring to those of `to`.
fn rebound(t: &mut Typer<'_>, x: TypeId, from: Binder, to: Binder) -> TypeId {
    t.u.map_type(x, &mut |u, part| match *u.types().get(part) {
        Type::Param(b, i) if b == from => Some(u.intern_type(Type::Param(to, i))),
        _ => None,
    })
}
