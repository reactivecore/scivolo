// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The parent type a function value of a single abstract method type gets (dotty's
//! `SAMType.samParent`).

use super::Typer;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether the refinements of `refined` over `unrefined` all refine terms.
    pub fn only_term_refinements(&mut self, mut refined: TypeId, unrefined: TypeId) -> bool {
        while refined != unrefined {
            let Type::Refined(p, _, info) = *self.u.ty(refined) else { return false };
            if matches!(self.u.ty(info), Type::Alias(_) | Type::Bounds(..)) {
                return false;
            }
            refined = p;
        }
        true
    }

    /// `parent`, an application of class `c`, its wildcard arguments replaced by their lower
    /// bound where the type parameter is a parameter type of `method` and occurs nowhere else,
    /// by their upper bound otherwise and where `method` is inherited (dotty's `samParent`);
    /// `None` for an occurrence in a form not followed.
    pub fn without_wildcard_args(&mut self, parent: TypeId, c: SymbolId, method: SymbolId) -> Option<TypeId> {
        let Some((tycon, args)) = self.u.applied(parent).map(|(tycon, args)| (tycon, args.to_vec())) else {
            return Some(parent);
        };
        if !args.iter().any(|&a| self.u.bounds(a).is_some()) {
            return Some(parent);
        }
        let declared = self.u.symbol(method).owner == Some(c);
        let tparams = self.u.type_params(c);
        let info = self.u.info(method);
        let mut new_args = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            let Some((lo, hi)) = self.u.bounds(a) else {
                new_args.push(a);
                continue;
            };
            let p = tparams.get(i).copied()?;
            let mut seen = (false, false);
            if declared {
                self.occurrences(info, p, 1, &mut seen).then_some(())?;
            }
            new_args.push(if seen == (false, true) { lo } else { hi });
        }
        Some(self.u.intern_applied(tycon, &new_args))
    }

    /// Records in `seen` whether type parameter `p` occurs in `t` at variance `v` covariantly and
    /// whether contravariantly, an invariant occurrence as both; `false` for a form not followed.
    fn occurrences(&mut self, t: TypeId, p: SymbolId, v: i8, seen: &mut (bool, bool)) -> bool {
        match *self.u.ty(t) {
            Type::Sym(_, s) if s == p => {
                seen.0 |= v >= 0;
                seen.1 |= v <= 0;
                true
            }
            Type::Applied(tycon, args) => {
                let Some(class) = self.u.class_symbol(tycon) else { return !self.refers_to(t, &[p]) };
                let variances = self.u.variances(class);
                (0..self.u.types_of(args).len()).all(|i| {
                    let w = variances.get(i).copied().unwrap_or(0);
                    self.occurrences(self.u.types_of(args)[i], p, v * w, seen)
                })
            }
            Type::Method { params, result, .. } => {
                (0..self.u.params_of(params).len())
                    .all(|i| self.occurrences(self.u.params_of(params)[i].1, p, -v, seen))
                    && self.occurrences(result, p, v, seen)
            }
            Type::Bounds(lo, hi) => self.occurrences(lo, p, -v, seen) && self.occurrences(hi, p, v, seen),
            Type::ByName(r) | Type::Flexible(r) => self.occurrences(r, p, v, seen),
            Type::And(a, b) | Type::Or(a, b, _) => {
                self.occurrences(a, p, v, seen) && self.occurrences(b, p, v, seen)
            }
            _ => !self.refers_to(t, &[p]),
        }
    }
}
