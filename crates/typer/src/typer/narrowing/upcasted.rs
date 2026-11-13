// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//! A pattern type constrained by a selector type upcast to what both may share (dotty's
//! `constrainUpcasted`).

use super::super::Typer;
use super::pattern_type::{class_of, constrain_pattern_type, try_constrain_simple};
use tasty::{SymbolId, Type, TypeId};

/// Constrains `pat` by `scrut` upcast: a class by the intersection of its parents, a class
/// application by its base types at the nearest parents `pat`'s class derives from, an abstract
/// type by its GADT upper bound and then its super type; true where nothing is left to compare.
pub fn constrain_upcasted(t: &mut Typer, pat: TypeId, scrut: TypeId) -> bool {
    let ty = *t.u.ty(scrut);
    let class_tycon = |t: &mut Typer, tycon| t.u.sym_of(tycon).filter(|&c| t.u.is_class(c));
    match ty {
        Type::Sym(_, c) if t.u.is_class(c) => {
            let parents = t.u.parents(c);
            and_of(t, parents).is_none_or(|and| constrain_pattern_type(t, pat, and))
        }
        Type::Applied(tycon, _) if class_tycon(t, tycon).is_some() => {
            let Some(cls) = class_tycon(t, tycon) else { return true };
            let pat_cls = class_of(t, pat);
            let mut shared = Vec::new();
            parents_shared_with(t, cls, pat_cls, &mut shared);
            let bases: Vec<TypeId> = shared.into_iter().filter_map(|c| t.u.base_type(scrut, c)).collect();
            and_of(t, bases).is_none_or(|and| constrain_pattern_type(t, pat, and))
        }
        _ => gadt_bounds_upcast(t, pat, scrut) && super_type_upcast(t, pat, scrut),
    }
}

/// Constrains `pat` by the GADT upper bound of the registered symbol `scrut` is (dotty's
/// `tryGadtBounds`).
fn gadt_bounds_upcast(t: &mut Typer, pat: TypeId, scrut: TypeId) -> bool {
    let Some((_, s)) = t.u.sym_ref(scrut) else { return true };
    match t.u.gadt().bounds(s) {
        Some((_, hi)) => constrain_pattern_type(t, pat, hi),
        None => true,
    }
}

/// Constrains `pat` by the super type of `scrut`, an abstract type's upper bound or a
/// singleton's underlying type (dotty's `trySuperType`).
fn super_type_upcast(t: &mut Typer, pat: TypeId, scrut: TypeId) -> bool {
    let ty = *t.u.ty(scrut);
    let upcast = match ty {
        Type::Annotated(inner, _) => Some(inner),
        Type::Sym(..)
        | Type::TypeRef(..)
        | Type::TermRef(..)
        | Type::This(_)
        | Type::Constant(..)
        | Type::Str(_)
        | Type::Skolem(..)
            if t.is_singleton(scrut) =>
        {
            Some(t.u.widen(scrut))
        }
        Type::Sym(..) | Type::TypeRef(..) => {
            let s = t.u.sym_of(scrut).or_else(|| t.u.resolve_ref(scrut));
            s.and_then(|s| {
                let info = t.u.info(s);
                t.u.bounds(info).map(|(_, hi)| hi)
            })
        }
        _ => None,
    };
    match upcast.filter(|&u| u != scrut) {
        Some(up) => try_constrain_simple(t, pat, up) || constrain_upcasted(t, pat, up),
        None => true,
    }
}

/// Folds `types` into an intersection from the left; `None` for no types (dotty's
/// `buildAndType`).
fn and_of(t: &mut Typer, types: Vec<TypeId>) -> Option<TypeId> {
    types.into_iter().reduce(|acc, x| t.u.intern_type(Type::And(acc, x)))
}

/// Adds the nearest base classes of class `c` that `pat_cls` derives from to `out`, a first
/// parent `Object` left out (dotty's `allParentsSharedWithPat`).
fn parents_shared_with(t: &mut Typer, c: SymbolId, pat_cls: Option<SymbolId>, out: &mut Vec<SymbolId>) {
    let parents = t.u.parents(c);
    for (i, p) in parents.into_iter().enumerate() {
        let Some(k) = class_of(t, p) else { continue };
        if i == 0 && t.u.full_name(k) == crate::defn::OBJECT {
            continue;
        }
        if pat_cls.is_some_and(|pc| t.u.derives_from(pc, k)) {
            out.push(k);
        } else {
            parents_shared_with(t, k, pat_cls, out);
        }
    }
}
