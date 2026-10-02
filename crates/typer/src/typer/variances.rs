//! Where type variables occur in a type: covariantly, contravariantly or both (dotty's
//! `variances`).

use super::Typer;
use rustc_hash::FxHashMap;
use tasty::{Type, TypeId, TypeVars};

impl Typer<'_> {
    /// The variance of each type variable of id `from` or later occurring in `t`: 1 covariant,
    /// -1 contravariant, 0 both or invariant.
    pub fn variances(&mut self, t: TypeId, from: usize) -> FxHashMap<u32, i8> {
        let mut out = FxHashMap::default();
        self.collect_variances(t, 1, from, &mut out, 0);
        out
    }

    fn collect_variances(&mut self, t: TypeId, v: i8, from: usize, out: &mut FxHashMap<u32, i8>, depth: u32) {
        if depth > 32 {
            return;
        }
        let mut go = |s: &mut Self, t, v| s.collect_variances(t, v, from, out, depth + 1);
        match self.u.types().get(t).clone() {
            Type::Var(var) if var.0 as usize >= from => match self.constraint.instance(var) {
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
                go(self, tycon, 0);
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
                for var in vars.into_iter().filter(|var| var.0 as usize >= from) {
                    let var_t = self.u.intern_type(Type::Var(var));
                    go(self, var_t, 0);
                }
            }
        }
    }
}
