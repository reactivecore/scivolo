// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Ranking successful implicit candidates (dotty's `compare` with `preferGeneral`): by the
//! derivation of their owners and by their types, first without their implicit clauses, then
//! with them; the same member by its prefix.

use super::super::Typer;
use super::super::type_ops::TypeOps;
use super::alt_types::{alt_type, parents_of};
use super::as_good::{is_as_good, relaxed_conforms};
use super::candidates::{Candidate, implicit_members};
use crate::ctx::Ctx;
use tasty::{SymbolId, Type, TypeId, Universe};

/// The candidate of `found` that ranks above all others in `ctx`.
pub fn most_specific(t: &mut Typer<'_>, found: &[Candidate], ctx: Ctx) -> Option<Candidate> {
    let mut alts: Vec<Candidate> = Vec::new();
    for &c in found {
        if !alts.contains(&c) {
            alts.push(c);
        }
    }
    if let [only] = alts[..] {
        return Some(only);
    }
    alts.iter()
        .copied()
        .find(|&c| alts.iter().filter(|&&d| d != c).all(|&d| compare(t, c, d, ctx) == Some(1)))
}

/// 1 if `a` ranks above `b`, -1 if below, 0 if neither; `None` if undecided.
pub fn compare(t: &mut Typer<'_>, a: Candidate, b: Candidate, ctx: Ctx) -> Option<i32> {
    let (full_a, full_b) = (alt_type(t, a), alt_type(t, b));
    let (sa, sb) = (without_implicits(t.u, full_a)?, without_implicits(t.u, full_b)?);
    let result = compare_with_types(t, (a, sa), (b, sb), ctx)?;
    Some(match (result, sa == full_a, sb == full_b) {
        (0, true, true) => 0,
        (0, true, false) => 1,
        (0, false, true) => -1,
        (0, false, false) => compare_with_types(t, (a, full_a), (b, full_b), ctx)?,
        _ => result,
    })
}

fn compare_with_types(
    t: &mut Typer<'_>,
    (a, ta): (Candidate, TypeId),
    (b, tb): (Candidate, TypeId),
    ctx: Ctx,
) -> Option<i32> {
    let owner = compare_owner(t, t.u.symbol(a.sym).owner, t.u.symbol(b.sym).owner);
    let wins_a = is_as_good(t, (a, ta), (b, tb), ctx)?;
    let wins_b = is_as_good(t, (b, tb), (a, ta), ctx)?;
    let mut ty = t.types();
    let same = |ty: &mut TypeOps<'_>| {
        let (ra, rb) = (ty.ref_type(a.prefix, a.sym), ty.ref_type(b.prefix, b.sym));
        a.sym == b.sym && ty.u.is_subtype(ra, rb) && ty.u.is_subtype(rb, ra)
    };
    let stable = ty.u.symbol(a.sym).is_stable_val();
    if wins_a && wins_b && stable && (ty.is_singleton(ta) || ty.is_singleton(tb) || same(&mut ty)) {
        return None;
    }
    let implicit = |c: Candidate| ty.u.symbol(c.sym).is_old_implicit();
    let new_scheme = !(implicit(a) && implicit(b));
    Some(match owner {
        1 if wins_a || !wins_b || new_scheme => 1,
        -1 if wins_b || !wins_a || new_scheme => -1,
        1 | -1 => 0,
        _ if wins_a != wins_b => i32::from(wins_a) - i32::from(wins_b),
        _ if a.sym == b.sym => compare_prefixes(&mut t.types(), a, b),
        _ => 0,
    })
}

/// `t` without the implicit clauses that lead it or its polymorphic result (dotty's
/// `stripImplicit`); `None` for a result depending on a stripped parameter.
fn without_implicits(u: &mut Universe, t: TypeId) -> Option<TypeId> {
    match u.types().get(t).clone() {
        Type::Method { binder, result, implicit: true, .. } => {
            let mut dependent = false;
            u.map_type(result, &mut |u, p| {
                dependent |= matches!(u.types().get(p), Type::Param(b, _) if *b == binder);
                None
            });
            if dependent { None } else { without_implicits(u, result) }
        }
        Type::Poly { binder, params, result } => {
            let stripped = without_implicits(u, result)?;
            match u.types().get(stripped) {
                _ if stripped == result => Some(t),
                Type::Poly { .. } => None,
                _ => Some(u.intern_type(Type::Poly { binder, params, result: stripped })),
            }
        }
        _ => Some(t),
    }
}

/// 1 if owner `x` derives from owner `y`, -1 the other way round, an object counting as its
/// companion class when the larger of the two objects inherits no implicits; else 0 (dotty's
/// `compareOwner`).
fn compare_owner(t: &mut Typer<'_>, x: Option<SymbolId>, y: Option<SymbolId>) -> i32 {
    let (Some(x), Some(y)) = (x, y) else { return 0 };
    if x == y {
        return 0;
    }
    if t.u.is_class(x) && t.u.is_class(y) {
        if t.u.derives_from(x, y) {
            return 1;
        }
        if t.u.derives_from(y, x) {
            return -1;
        }
    }
    match (t.u.is_module_class(x), t.u.is_module_class(y)) {
        (true, true) => {
            let (cx, cy) = (t.u.companion_class(x), t.u.companion_class(y));
            let r = compare_owner(t, cx, cy);
            let larger = if r < 0 { x } else { y };
            let own = implicit_members(t, larger, true).iter().all(|&m| t.u.symbol(m).owner == Some(larger));
            if own { r } else { 0 }
        }
        (true, false) => {
            let cx = t.u.companion_class(x);
            compare_owner(t, cx, Some(y))
        }
        (false, true) => {
            let cy = t.u.companion_class(y);
            compare_owner(t, Some(x), cy)
        }
        _ => 0,
    }
}

/// For two references to one member: 1 if `a`'s prefix is the more specific, -1 if `b`'s, an
/// object's prefix counting as its parents (dotty's `comparePrefixes`).
fn compare_prefixes(ty: &mut TypeOps<'_>, a: Candidate, b: Candidate) -> i32 {
    let (pa, pb) = (widened_prefix(ty.u, a.prefix), widened_prefix(ty.u, b.prefix));
    let before = ty.constraint.attempt();
    let wins_a = relaxed_conforms(ty, pa, pb);
    ty.constraint.abandon(before);
    let wins_b = relaxed_conforms(ty, pb, pa);
    ty.constraint.abandon(before);
    i32::from(wins_a) - i32::from(wins_b)
}

fn widened_prefix(u: &mut Universe, prefix: Option<TypeId>) -> TypeId {
    let Some(p) = prefix else { return u.intern_type(Type::NoPrefix) };
    let w = u.widen(p);
    match u.class_symbol(w) {
        Some(c) if u.is_module_class(c) && !u.symbol(c).is_package() => parents_of(u, w, c),
        _ => w,
    }
}
