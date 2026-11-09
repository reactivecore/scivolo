// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Whether trying a candidate would make the implicit search diverge (dotty's
//! `checkDivergence`).

use super::super::Typer;
use super::super::type_ops::TypeOps;
use super::candidates::Candidate;
use super::measures::Measure;
use super::search::ImplicitSearch;
use tasty::{Binder, Type, TypeId};

impl ImplicitSearch {
    /// Whether candidate `c` diverges: an equivalent candidate is open on a type no smaller built
    /// from the same symbols, and no by-name argument lies between to tie a knot.
    pub fn diverges(&self, t: &mut Typer<'_>, c: Candidate) -> Result<bool, &'static str> {
        let pt = self.pt;
        let mut below = self.by_name;
        let mut current: Option<Measure> = None;
        for i in (0..t.history.open.len()).rev() {
            let (cand, tp, tp_by_name, view) = {
                let o = &t.history.open[i];
                (o.cand, o.pt, o.by_name, o.view)
            };
            if equivalent(t, cand, c) {
                if view {
                    return Err("divergence check through a conversion");
                }
                let mut ty = t.types();
                let (wild_tp, wild_pt) = (ty.wild_approx(tp), ty.wild_approx(pt));
                if below && ty.frozen_conforms(wild_tp, wild_pt) {
                    t.fully_defined_as(tp, true)?;
                    return Ok(false);
                }
                let prev = match t.history.open[i].measure.clone() {
                    Some(m) => m,
                    None => Measure::of(&mut t.types(), tp)?,
                };
                t.history.open[i].measure = Some(prev.clone());
                let mut ty = t.types();
                let cur = match current.clone() {
                    Some(m) => m,
                    None => Measure::of(&mut ty, pt)?,
                };
                current = Some(cur.clone());
                if prev.size <= cur.size
                    && prev.covering == cur.covering
                    && (prev.size < cur.size || frozen_same(&mut ty, wild_tp, wild_pt))
                {
                    return Ok(true);
                }
            }
            below |= tp_by_name;
        }
        Ok(false)
    }
}

fn frozen_same(ty: &mut TypeOps<'_>, a: TypeId, b: TypeId) -> bool {
    a == b || ty.frozen_conforms(a, b) && ty.frozen_conforms(b, a)
}

/// Whether `a` and `b` are the same reference, or siblings of the same type (dotty's
/// `equivalentCandidates`).
fn equivalent(t: &mut Typer<'_>, a: Candidate, b: Candidate) -> bool {
    if a == b {
        return true;
    }
    if t.u.symbol(a.sym).owner != t.u.symbol(b.sym).owner {
        return false;
    }
    let (ta, tb) = (t.types().ref_type(a.prefix, a.sym), t.types().ref_type(b.prefix, b.sym));
    let ta = t.method_info(ta).unwrap_or(ta);
    let tb = t.method_info(tb).unwrap_or(tb);
    same_signature(&mut t.types(), ta, tb)
}

/// Whether method or value types `a` and `b` are the same, their parameters matched by position.
fn same_signature(ty: &mut TypeOps<'_>, a: TypeId, b: TypeId) -> bool {
    let (a, b) = (ty.zonk(a), ty.zonk(b));
    let sides = (ty.u.types().get(a).clone(), ty.u.types().get(b).clone());
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
        _ => return frozen_same(ty, a, b),
    };
    if pa.len() != pb.len() {
        return false;
    }
    for ((_, x), (_, y)) in pa.into_iter().zip(pb) {
        let y = rebound(ty.u, y, bb, ba);
        if !frozen_same(ty, x, y) {
            return false;
        }
    }
    let rb = rebound(ty.u, rb, bb, ba);
    same_signature(ty, ra, rb)
}

/// `x` with the parameters of binder `from` referring to those of `to`.
fn rebound(u: &mut tasty::Universe, x: TypeId, from: Binder, to: Binder) -> TypeId {
    u.map_type(x, &mut |u, part| match *u.types().get(part) {
        Type::Param(b, i) if b == from => Some(u.intern_type(Type::Param(to, i))),
        _ => None,
    })
}
