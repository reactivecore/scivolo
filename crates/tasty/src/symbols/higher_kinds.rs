// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Variances.scala
//   compiler/src/dotty/tools/dotc/core/TypeApplications.scala
//! Type constructors that are type lambdas: whether a constructor is one, and the variances of
//! a lambda's parameters.

use super::{Kind, Universe};
use crate::{Binder, Type, TypeId};

impl Universe {
    /// Whether type constructor `tycon` is a type lambda or bounded by one, and not a match alias
    /// (dotty's `isLambdaSub && !isMatchAlias`).
    pub fn is_lambda_sub(&mut self, tycon: TypeId) -> bool {
        let t = self.dealias(tycon);
        match *self.types().get(t) {
            Type::Lambda { result, .. } => !matches!(self.types().get(result), Type::Match { .. }),
            Type::Sym(..) | Type::TypeRef(..) => {
                let Some(s) = self.referenced(t) else { return false };
                if !matches!(self.symbol(s).kind, Kind::Type | Kind::TypeParam) {
                    return false;
                }
                let info = self.info(s);
                match *self.types().get(info) {
                    Type::Bounds(_, hi) if hi != t => self.is_lambda_sub(hi),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// The variances of the `n` parameters of type lambda `binder` over `result`: declared, else
    /// structural, a parameter that does not occur covariant (dotty's `paramVarianceSign`).
    pub fn lambda_param_variances(&mut self, binder: Binder, n: usize, result: TypeId) -> Vec<i8> {
        if let Some(declared) = self.declared_variances(binder) {
            return declared;
        }
        let mut seen = vec![0u8; n];
        self.occurrences(result, binder, 1, &mut seen);
        seen.into_iter().map(|s| [1, 1, -1, 0][s as usize]).collect()
    }

    /// Marks in `seen` how parameters of `binder` occur in `t` at variance `v`: bit 1
    /// covariantly, bit 2 contravariantly.
    fn occurrences(&mut self, t: TypeId, binder: Binder, v: i8, seen: &mut [u8]) {
        match self.types().get(t).clone() {
            Type::Param(b, i) if b == binder => {
                if let Some(s) = seen.get_mut(i as usize) {
                    *s |= match v {
                        1 => 1,
                        -1 => 2,
                        _ => 3,
                    };
                }
            }
            Type::Applied(tycon, args) => {
                let variances = self.constructor_variances(tycon);
                for (i, a) in args.into_iter().enumerate() {
                    self.occurrences(a, binder, v * variances.get(i).copied().unwrap_or(0), seen);
                }
                self.occurrences(tycon, binder, v, seen);
            }
            Type::Bounds(lo, hi) => {
                self.occurrences(lo, binder, -v, seen);
                self.occurrences(hi, binder, v, seen);
            }
            Type::And(a, b) | Type::Or(a, b, _) => {
                self.occurrences(a, binder, v, seen);
                self.occurrences(b, binder, v, seen);
            }
            Type::Annotated(a, _) | Type::ByName(a) => self.occurrences(a, binder, v, seen),
            _ => {
                let mut parts = Vec::new();
                self.map_type(t, &mut |_, x| {
                    if x == t {
                        return None;
                    }
                    parts.push(x);
                    Some(x)
                });
                for p in parts {
                    self.occurrences(p, binder, 0, seen);
                }
            }
        }
    }
}
