// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Variances.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! `TypeLambda.paramVariances`: declared variances, else those of the parameters' occurrences in
//! the body (dotty's `setStructuralVariances`), as flags: covariant 1, contravariant 2, both
//! (bivariant) 3, invariant 0.

use super::Fallible;
use crate::host::Host;
use tasty::{Binder, Type, TypeId, Universe};

impl Host {
    pub fn param_variances(&mut self, t: TypeId) -> Fallible<Vec<i32>> {
        lambda_variances(&mut self.u, t).ok_or("TypeLambda.paramVariances of another type")
    }
}

fn lambda_variances(u: &mut Universe, t: TypeId) -> Option<Vec<i32>> {
    let Type::Lambda { binder, params, result } = u.types().get(t).clone() else { return None };
    if let Some(declared) = u.declared_variances(binder).filter(|v| v.len() == params.len()) {
        return Some(declared.into_iter().map(flags).collect());
    }
    let mut narrowing = Narrowing { binder, found: vec![3; params.len()], params };
    narrowing.go(u, result, 1);
    Some(narrowing.found)
}

fn flags(sign: i8) -> i32 {
    match sign {
        1 => 1,
        -1 => 2,
        _ => 0,
    }
}

/// The variances of the parameters of a type lambda, narrowed by their occurrences.
struct Narrowing {
    binder: Binder,
    params: Vec<(String, TypeId)>,
    found: Vec<i32>,
}

impl Narrowing {
    /// Narrows the variances by the occurrences in `t`, at variance `v`.
    fn go(&mut self, u: &mut Universe, t: TypeId, v: i8) {
        match u.types().get(t).clone() {
            Type::Param(b, i) if b == self.binder => {
                if let Some(f) = self.found.get_mut(i as usize) {
                    *f &= flags(v);
                }
            }
            Type::Sym(Some(prefix), _) | Type::TypeRef(prefix, _) | Type::TermRef(prefix, _) => {
                self.go(u, prefix, v.max(0))
            }
            Type::Applied(tycon, args) => {
                self.go(u, tycon, v);
                let signs = self.tycon_variances(u, tycon, args.len());
                for (arg, s) in args.into_iter().zip(signs) {
                    let bounds = matches!(u.types().get(arg), Type::Bounds(..) | Type::Wildcard(_));
                    self.go(u, arg, if bounds { v } else { v * s });
                }
            }
            Type::Bounds(lo, hi) => {
                self.go(u, lo, -v);
                self.go(u, hi, v);
            }
            Type::Alias(a) => self.go(u, a, 0),
            Type::Wildcard(Some(b)) | Type::ByName(b) | Type::Annotated(b, _) | Type::Rec(_, b) => {
                self.go(u, b, v)
            }
            Type::And(a, b) | Type::Or(a, b, _) | Type::Refined(a, _, b) => {
                self.go(u, a, v);
                self.go(u, b, v);
            }
            Type::Method { params, result, .. }
            | Type::Poly { params, result, .. }
            | Type::Lambda { params, result, .. } => {
                for (_, p) in params {
                    self.go(u, p, -v);
                }
                self.go(u, result, v);
            }
            Type::Match { bound, scrutinee, cases } => {
                self.go(u, bound, v);
                for c in [scrutinee].into_iter().chain(cases) {
                    self.go(u, c, 0);
                }
            }
            _ => {}
        }
    }

    /// The variance signs of the `n` type parameters of `tycon`, invariant where unknown; a
    /// parameter of the lambda has those of the type lambda bounding it.
    fn tycon_variances(&self, u: &mut Universe, tycon: TypeId, n: usize) -> Vec<i8> {
        let signs = match *u.types().get(tycon) {
            Type::Param(b, i) if b == self.binder => {
                let bound = self.params.get(i as usize).map(|&(_, p)| upper(u, p));
                let found = bound.and_then(|b| lambda_variances(u, b)).unwrap_or_default();
                found.into_iter().map(|f| [0, 1, -1, 1][f as usize]).collect()
            }
            _ => u.constructor_variances(tycon),
        };
        if signs.len() == n { signs } else { vec![0; n] }
    }
}

/// The upper bound of bounds `t`, else `t`.
fn upper(u: &Universe, t: TypeId) -> TypeId {
    match *u.types().get(t) {
        Type::Bounds(_, hi) => hi,
        _ => t,
    }
}
