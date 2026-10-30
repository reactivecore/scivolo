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
        lambda_variances(&mut self.u, t, &[]).ok_or("TypeLambda.paramVariances of another type")
    }
}

/// The variances of the parameters of type lambda `t`; bivariant for a lambda of `pending`,
/// whose variances are being computed.
fn lambda_variances(u: &mut Universe, t: TypeId, pending: &[Binder]) -> Option<Vec<i32>> {
    let Type::Lambda { binder, params, result } = u.types().get(t).clone() else { return None };
    if let Some(declared) = u.declared_variances(binder).filter(|v| v.len() == params.len()) {
        return Some(declared.into_iter().map(flags).collect());
    }
    if pending.contains(&binder) {
        return Some(vec![3; params.len()]);
    }
    let pending = [pending, &[binder]].concat();
    let mut narrowing = Narrowing { binder, found: vec![3; params.len()], params, pending };
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
    pending: Vec<Binder>,
}

impl Narrowing {
    /// Narrows the variances by the occurrences in `t`, at variance `v`.
    fn go(&mut self, u: &mut Universe, t: TypeId, v: i8) {
        if let Some(i) = self.param(u, t) {
            if let Some(f) = self.found.get_mut(i) {
                *f &= flags(v);
            }
            return;
        }
        match u.types().get(t).clone() {
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
                let case = matches!(u.types().get(result), Type::MatchCase(..));
                for (_, p) in params {
                    self.go(u, p, if case { 0 } else { -v });
                }
                self.go(u, result, v);
            }
            Type::Match { bound, scrutinee, cases } => {
                self.go(u, bound, v);
                self.go(u, scrutinee, 0);
                for c in cases {
                    self.go(u, c, v);
                }
            }
            Type::MatchCase(pattern, body) => {
                self.go(u, pattern, 0);
                self.go(u, body, v);
            }
            _ => {}
        }
    }

    /// The index of the parameter `t` refers to, if it refers to one; a lambda read from TASTy
    /// refers to its parameters by name.
    fn param(&self, u: &Universe, t: TypeId) -> Option<usize> {
        match u.types().get(t) {
            Type::Param(b, i) if *b == self.binder => Some(*i as usize),
            Type::Local(b, n) if b.0 == self.binder.0 => self.params.iter().position(|(m, _)| m == n),
            _ => None,
        }
    }

    /// The variance signs of the `n` type parameters of `tycon`, invariant where unknown: those of
    /// the type lambda bounding a parameter of the lambda or an alias stands for, else those
    /// declared.
    fn tycon_variances(&self, u: &mut Universe, tycon: TypeId, n: usize) -> Vec<i8> {
        let lambda = match self.param(u, tycon) {
            Some(i) => self.params.get(i).map(|&(_, p)| upper(u, p)),
            None => u.referenced_alias(tycon),
        };
        let signs = match lambda.and_then(|l| lambda_variances(u, l, &self.pending)) {
            Some(found) => found.into_iter().map(|f| [0, 1, -1, 1][f as usize]).collect(),
            None => u.constructor_variances(tycon),
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
