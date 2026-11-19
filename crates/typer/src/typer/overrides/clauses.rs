// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! The parameter clauses of an overriding and an overridden method compared as dotty's
//! `compareMethod` and `comparePoly` do.

use super::super::Typer;
use tasty::{Binder, NameId, SymbolId, Type, TypeId};

/// A parameter clause of a method type.
pub struct Clause {
    pub is_type: bool,
    pub implicit: bool,
    pub binder: Binder,
    pub params: Vec<(NameId, TypeId)>,
    pub result: TypeId,
}

/// The leading parameter clause of method type `info`, if it has one.
pub fn clause(t: &Typer<'_>, info: TypeId) -> Option<Clause> {
    let (is_type, implicit, binder, params, result) = match *t.u.ty(info) {
        Type::Poly { binder, params, result } => (true, false, binder, params, result),
        Type::Method { binder, params, result, implicit, .. } => (false, implicit, binder, params, result),
        _ => return None,
    };
    Some(Clause { is_type, implicit, binder, params: t.u.params_of(params).to_vec(), result })
}

/// Whether clause `a` of an overriding method may override clause `b`, both of a kind and of a
/// length: an implicit or `using` clause only by another, type parameters only by ones whose
/// bounds contain `b`'s; `own` holds the type parameter symbols of `a` and of `b`.
pub fn clause_overrides(t: &mut Typer<'_>, a: &Clause, b: &Clause, own: (&[SymbolId], &[SymbolId])) -> bool {
    if b.implicit && !a.implicit {
        return false;
    }
    if !a.is_type {
        return true;
    }
    let pairs: Vec<(TypeId, TypeId)> =
        a.params.iter().zip(&b.params).map(|(&(_, x), &(_, y))| (x, y)).collect();
    pairs.into_iter().all(|(ia, ib)| {
        let ib = t.u.with_binder(ib, (b.binder, &b.params), a.binder);
        let renamed = (type_param_refs(t, ia, own.0, a.binder), type_param_refs(t, ib, own.1, a.binder));
        bounds_contain(t, renamed) || bounds_contain(t, (ia, ib))
    })
}

/// Whether bounds `infos.0` contain bounds `infos.1`.
fn bounds_contain(t: &mut Typer<'_>, (ia, ib): (TypeId, TypeId)) -> bool {
    let (Some((lo_a, hi_a)), Some((lo_b, hi_b))) = (split_bounds(t, ia), split_bounds(t, ib)) else {
        return true;
    };
    t.conforms(lo_a, lo_b) && t.conforms(hi_b, hi_a)
}

/// The lower and upper bound of type member or parameter info `info`, those of a higher-kinded
/// one as type lambdas.
pub fn split_bounds(t: &mut Typer<'_>, info: TypeId) -> Option<(TypeId, TypeId)> {
    match *t.u.ty(info) {
        Type::Alias(a) => Some((a, a)),
        Type::Bounds(lo, hi) => Some((lo, hi)),
        Type::Lambda { binder, params, result } => Some(match t.u.bounds(result) {
            Some((lo, hi)) => {
                let mut lambda = |x| t.u.intern_type(Type::Lambda { binder, params, result: x });
                (lambda(lo), lambda(hi))
            }
            None => (info, info),
        }),
        _ => None,
    }
}

/// `ty` with references to type parameters `params` made references to parameters of `binder`.
pub fn type_param_refs(t: &mut Typer<'_>, ty: TypeId, params: &[SymbolId], binder: Binder) -> TypeId {
    t.u.map_type(ty, &mut |u, x| {
        let Some((None, s)) = u.sym_ref(x) else { return None };
        params.iter().position(|&p| p == s).map(|i| u.intern_type(Type::Param(binder, i as u32)))
    })
}
