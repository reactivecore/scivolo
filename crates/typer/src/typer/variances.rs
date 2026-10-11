//! Where type variables occur in a type: covariantly, contravariantly or both (dotty's
//! `variances`).

use super::Typer;
use rustc_hash::FxHashMap;
use tasty::{Type, TypeId, TypeVars, VarId};

impl Typer<'_> {
    /// The variance of each type variable of id `from` or later occurring in `t` or in the final
    /// result type of `pt`, or in the bounds of such a variable: 1 covariant, -1 contravariant,
    /// 0 both or invariant.
    pub fn variances(&mut self, t: TypeId, pt: Option<TypeId>, from: usize) -> FxHashMap<u32, i8> {
        let mut out = FxHashMap::default();
        self.collect_variances(t, 1, &mut out, 0);
        if let Some(pt) = pt {
            let result = self.u.result_type(pt);
            self.collect_variances(result, 1, &mut out, 0);
        }
        let (below, above) = self.var_bounded_by();
        let mut work: Vec<u32> = out.keys().copied().collect();
        while let Some(var) = work.pop() {
            let v = out[&var];
            let (lo, hi) = self.constraint.bounds(VarId(var));
            let mut found = FxHashMap::default();
            if v >= 0 {
                self.collect_variances(lo, v, &mut found, 0);
                for &w in below.get(&var).into_iter().flatten() {
                    found.insert(w, v);
                }
            }
            if v <= 0 {
                self.collect_variances(hi, v, &mut found, 0);
                for &w in above.get(&var).into_iter().flatten() {
                    found.insert(w, v);
                }
            }
            for (w, x) in found {
                let prev = out.get(&w).copied();
                let new = match prev {
                    Some(p) if p != x => 0,
                    _ => x,
                };
                if prev != Some(new) {
                    out.insert(w, new);
                    work.push(w);
                }
            }
        }
        out.retain(|&var, _| var as usize >= from);
        out
    }

    /// For each type variable, the uninstantiated ones that bound it directly from below and
    /// from above, by the intersection operands of their upper and the union operands of their
    /// lower bounds (the other direction of dotty's `isLess`).
    fn var_bounded_by(&mut self) -> (FxHashMap<u32, Vec<u32>>, FxHashMap<u32, Vec<u32>>) {
        let (mut below, mut above) = (FxHashMap::default(), FxHashMap::<u32, Vec<u32>>::default());
        for w in 0..self.constraint.count() as u32 {
            if self.constraint.instance(VarId(w)).is_some() {
                continue;
            }
            let (lo, hi) = self.constraint.bounds(VarId(w));
            for x in self.var_factors(hi, false) {
                below.entry(x).or_insert_with(Vec::new).push(w);
            }
            for x in self.var_factors(lo, true) {
                above.entry(x).or_insert_with(Vec::new).push(w);
            }
        }
        (below, above)
    }

    /// The variables among the top-level union (`or`) or intersection operands of `t`.
    fn var_factors(&self, t: TypeId, or: bool) -> Vec<u32> {
        let factors = self.factors(t, or);
        factors
            .into_iter()
            .filter_map(|f| match *self.u.types().get(f) {
                Type::Var(x) => Some(x.0),
                _ => None,
            })
            .collect()
    }

    /// The variance of each type variable occurring in `t`, as `variances` without bounds and
    /// expected type.
    pub fn occurrence_variances(&mut self, t: TypeId) -> FxHashMap<u32, i8> {
        let mut out = FxHashMap::default();
        self.collect_variances(t, 1, &mut out, 0);
        out
    }

    fn collect_variances(&mut self, t: TypeId, v: i8, out: &mut FxHashMap<u32, i8>, depth: u32) {
        if depth > 32 {
            return;
        }
        let mut go = |s: &mut Self, t, v| s.collect_variances(t, v, out, depth + 1);
        match self.u.types().get(t).clone() {
            Type::Var(var) => match self.constraint.instance(var) {
                Some(i) => go(self, i, v),
                None => {
                    let entry = out.entry(var.0).or_insert(v);
                    if *entry != v {
                        *entry = 0;
                    }
                }
            },
            Type::Applied(tycon, args) => {
                let params = self.u.class_symbol(tycon).map(|c| self.u.type_params(c)).unwrap_or_default();
                for (i, a) in args.into_iter().enumerate() {
                    let flags = params.get(i).map(|&p| self.u.symbol(p).flags);
                    let pv = match flags {
                        Some(f) if f.has(tasty::tags::COVARIANT) => 1,
                        Some(f) if f.has(tasty::tags::CONTRAVARIANT) => -1,
                        _ => 0,
                    };
                    go(self, a, v * pv);
                }
                go(self, tycon, v);
            }
            Type::Method { params, result, .. } => {
                for (_, p) in params {
                    go(self, p, -v);
                }
                go(self, result, v);
            }
            Type::Poly { result, .. } | Type::ByName(result) | Type::Annotated(result, _) => {
                go(self, result, v)
            }
            Type::And(a, b) | Type::Or(a, b) => {
                go(self, a, v);
                go(self, b, v);
            }
            Type::Bounds(lo, hi) => {
                go(self, lo, -v);
                go(self, hi, v);
            }
            _ => {
                let mut vars = Vec::new();
                self.u.map_type(t, &mut |u, part| {
                    if let Type::Var(var) = *u.types().get(part) {
                        vars.push(var);
                    }
                    None
                });
                for var in vars {
                    let var_t = self.u.intern_type(Type::Var(var));
                    go(self, var_t, 0);
                }
            }
        }
    }
}
