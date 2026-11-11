// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//! Instantiating the type variables a typed tree determines (dotty's `interpolateTypeVars` and
//! `instantiateTypeVars`).

use super::type_ops::TypeOps;
use super::{Pt, Typer};
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use tasty::{ANY, NOTHING, Type, TypeId, TypeVars, VarId};

/// The variances of the variables in a variable's lower and upper bound.
type BoundVariances = (FxHashMap<u32, i8>, FxHashMap<u32, i8>);

impl Typer<'_> {
    /// Instantiates the type variables of id `from` or later that `tree`'s type determines, its
    /// result constrained by `pt` first unless `tree` is an application: minimized where they
    /// occur covariantly, maximized where contravariantly, by their lower bound if they do not
    /// occur and have one; invariant ones stay, and all stay while `tree` is a method not applied
    /// to all its parameter clauses.
    pub fn interpolate(&mut self, tree: TNodeId, pt: Pt, from: usize) -> Result<(), &'static str> {
        let Some(unwidened) = self.tpd.get(tree).tpe else { return Ok(()) };
        let tpe = self.u.widen(unwidened);
        let tpe = self.u.by_name(tpe).unwrap_or(tpe);
        if from >= self.constraint.count()
            || matches!(self.u.ty(tpe), Type::Method { .. } | Type::Poly { .. })
        {
            return Ok(());
        }
        let pt = match pt {
            Pt::Type(p) => Some(p),
            _ => None,
        };
        if let Some(p) = pt
            && !matches!(self.tpd.tree(tree), T::Apply { .. })
        {
            self.constrain_result(tree, unwidened, p);
        }
        let variances = self.types().variances(tpe, pt, from);
        let mut pending = Vec::new();
        for v in self.constraint.pending_from(from) {
            let variance = match variances.get(&v.0) {
                Some(0) => continue,
                Some(&x) => x,
                None => 0,
            };
            pending.push((v, variance));
        }
        let pending = self.types().filter_by_deps(pending);
        let pending = pending
            .into_iter()
            .map(|(v, x)| (v, x == 1 || x == 0 && self.constraint.bounds(v).0 != NOTHING))
            .collect();
        self.instantiate_in_order(pending)
    }

    /// Instantiates `vars` from below or above as given, each after the others among its bound's
    /// or-factors (from below) or and-factors (from above) (dotty's `doInstantiate`).
    fn instantiate_in_order(&mut self, mut vars: Vec<(VarId, bool)>) -> Result<(), &'static str> {
        while !vars.is_empty() {
            let mut suspended = Vec::new();
            for i in 0..vars.len() {
                let (v, from_below) = vars[i];
                if !self.constraint.is_pending(v) {
                    continue;
                }
                let ty = self.types();
                let waits = vars[i + 1..]
                    .iter()
                    .any(|&(w, _)| if from_below { ty.is_less(w, v) } else { ty.is_less(v, w) });
                if waits {
                    suspended.push((v, from_below));
                } else {
                    self.instantiate_var(v, from_below)?;
                }
            }
            if suspended.len() == vars.len() {
                for (v, from_below) in suspended {
                    self.instantiate_var(v, from_below)?;
                }
                return Ok(());
            }
            vars = suspended;
        }
        Ok(())
    }

    /// Constrains `tree`'s type `tpe` to conform to `pt`, keeping the constraint only if it does
    /// (dotty's `constrainResult`).
    fn constrain_result(&mut self, tree: TNodeId, tpe: TypeId, pt: TypeId) {
        let unit = crate::defn::type_ref(self.u, self.defn.unit);
        if self.u.widen(pt) == unit || matches!(self.u.ty(pt), Type::Error) {
            return;
        }
        let pt = self.result_pt(self.tpd.get(tree).sym, pt);
        let tpe = self.u.by_name(tpe).unwrap_or(tpe);
        let mark = self.constraint.mark();
        if !self.conforms(tpe, pt) {
            self.constraint.rollback(mark);
        }
    }
}

impl TypeOps<'_> {
    /// `vars` without the ones whose instantiation would narrow the bounds of a variable not
    /// among them, a non-occurring one directed away from such a variable (dotty's
    /// `filterByDeps`).
    fn filter_by_deps(&mut self, mut vars: Vec<(VarId, i8)>) -> Vec<(VarId, i8)> {
        loop {
            let excluded: Vec<VarId> = vars.iter().map(|&(v, _)| v).collect();
            let others = self.bound_variances(&excluded);
            let mut next = Vec::new();
            for &(v, x) in &vars {
                let above_ok = !self.depends_on(v, &excluded, &others, true);
                let below_ok = !self.depends_on(v, &excluded, &others, false);
                match x {
                    0 if !above_ok => next.push((v, 1)),
                    0 if !below_ok => next.push((v, -1)),
                    -1 if !above_ok => {}
                    1 if !below_ok => {}
                    _ => next.push((v, x)),
                }
            }
            if next == vars {
                return vars;
            }
            vars = next;
        }
    }

    /// The variances of the variables in the lower and upper bounds of each uninstantiated
    /// variable not in `except`.
    fn bound_variances(&mut self, except: &[VarId]) -> Vec<BoundVariances> {
        let mut out = Vec::new();
        let pending: Vec<VarId> = self.constraint.pending_from(0).filter(|w| !except.contains(w)).collect();
        for w in pending {
            let (lo, hi) = self.constraint.bounds(w);
            if lo != NOTHING || hi != ANY {
                out.push((self.occurrence_variances(lo), self.occurrence_variances(hi)));
            }
        }
        out
    }

    /// Whether a variable other than `except`, with bound variances `others`, has `v` in a bound
    /// where making `v` larger (`co`) or smaller narrows it, or bounds `v` from above (`co`) or
    /// below (dotty's `dependsOn`).
    fn depends_on(&self, v: VarId, except: &[VarId], others: &[BoundVariances], co: bool) -> bool {
        let target = if co { 1 } else { -1 };
        let referenced = others.iter().any(|(lo, hi)| {
            lo.get(&v.0).is_some_and(|&x| x == target || x == 0)
                || hi.get(&v.0).is_some_and(|&x| -x == target || x == 0)
        });
        let (lo, hi) = self.constraint.bounds(v);
        referenced
            || self.any_factor(if co { hi } else { lo }, !co, &mut |f| {
                self.u.var(f).is_some_and(|w| !except.contains(&w) && self.constraint.is_pending(w))
            })
    }

    /// Checks whether uninstantiated variable `a` is bounded by `b` directly: `b` an intersection
    /// operand of `a`'s upper bound, or `a` a union operand of `b`'s lower bound (dotty's
    /// `isLess`).
    pub fn is_less(&self, a: VarId, b: VarId) -> bool {
        let (hi, lo) = (self.constraint.bounds(a).1, self.constraint.bounds(b).0);
        self.any_factor(hi, false, &mut |f| self.u.var(f) == Some(b))
            || self.any_factor(lo, true, &mut |f| self.u.var(f) == Some(a))
    }
}
