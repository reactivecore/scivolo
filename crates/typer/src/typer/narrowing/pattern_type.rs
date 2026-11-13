// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! The constraints a pattern's type matching a selector's type necessarily implies, the GADT
//! constraint among them (dotty's `constrainPatternType` in `Mode.GadtConstraintInference`).

use super::super::Typer;
use super::either::either;
use tasty::{SymbolId, Type, TypeId, TypeVars};

/// Constrains type variables and narrows the GADT constraint by what pattern type `pat`
/// matching a selector of type `scrut` implies: unions by either operand keeping only what both
/// need, intersections by both, refinements dropped, else by the base types pattern and
/// selector share (dotty's `constrainPatternType`); false if they cannot share an instance.
pub fn constrain_pattern_type(t: &mut Typer, pat: TypeId, scrut: TypeId) -> bool {
    let scrut = dealias_drop_non_module_refs(t, scrut);
    match *t.u.ty(scrut) {
        Type::Or(a, b, _) => {
            return either(t, |t| constrain_pattern_type(t, pat, a), |t| constrain_pattern_type(t, pat, b));
        }
        Type::And(a, b) => return constrain_pattern_type(t, pat, a) && constrain_pattern_type(t, pat, b),
        Type::Refined(..) | Type::Rec(..) => {
            let stripped = strip_refinement(t, scrut);
            return constrain_pattern_type(t, pat, stripped);
        }
        _ => {}
    }
    let pat = dealias_drop_non_module_refs(t, pat);
    match *t.u.ty(pat) {
        Type::Or(a, b, _) => {
            either(t, |t| constrain_pattern_type(t, a, scrut), |t| constrain_pattern_type(t, b, scrut))
        }
        Type::And(a, b) => constrain_pattern_type(t, a, scrut) && constrain_pattern_type(t, b, scrut),
        Type::Refined(..) | Type::Rec(..) => {
            let stripped = strip_refinement(t, pat);
            constrain_pattern_type(t, stripped, scrut)
        }
        _ => {
            try_constrain_simple(t, pat, scrut)
                || classes_may_be_compatible(t, pat, scrut)
                    && super::upcasted::constrain_upcasted(t, pat, scrut)
        }
    }
}

/// `tpe` without its refinements and recursive binders (dotty's `stripRefinement`).
fn strip_refinement(t: &mut Typer, mut tpe: TypeId) -> TypeId {
    while let Type::Refined(p, ..) | Type::Rec(_, p) = *t.u.ty(tpe) {
        tpe = p;
    }
    tpe
}

/// `a <: b` in GADT mode under the typer's constraint.
pub fn is_sub(t: &mut Typer, a: TypeId, b: TypeId) -> bool {
    t.u.constrain_gadt_in(a, b, &mut t.constraint)
}

/// `scrut` dealiased, a path to a value of no class replaced by its type, a flexible type by
/// its underlying one (dotty's `dealiasDropNonmoduleRefs`).
fn dealias_drop_non_module_refs(t: &mut Typer, scrut: TypeId) -> TypeId {
    let d = t.u.dealias(scrut);
    let ty = *t.u.ty(d);
    match ty {
        Type::TermRef(..) | Type::Sym(..) if t.is_singleton(d) => {
            let widened = t.u.widen(d);
            if class_of(t, widened).is_some() { d } else { widened }
        }
        Type::Flexible(inner) => dealias_drop_non_module_refs(t, inner),
        _ => d,
    }
}

/// The class of `t` (dotty's `classSymbol`).
pub fn class_of(t: &mut Typer, tpe: TypeId) -> Option<SymbolId> {
    let tpe = if t.is_singleton(tpe) { t.u.widen(tpe) } else { tpe };
    t.u.class_symbol(tpe).filter(|&c| t.u.is_class(c))
}

/// Whether the classes of `pat` and `scrut` may have a common instance (dotty's
/// `classesMayBeCompatible`).
fn classes_may_be_compatible(t: &mut Typer, pat: TypeId, scrut: TypeId) -> bool {
    let (Some(p), Some(s)) = (class_of(t, pat), class_of(t, scrut)) else { return true };
    t.types().classes_may_be_compatible(p, s)
}

/// Constrains `pat` by `scrut` through their common base class if one derives from the other
/// (dotty's `tryConstrainSimplePatternType`).
pub fn try_constrain_simple(t: &mut Typer, pat: TypeId, scrut: TypeId) -> bool {
    let (Some(p), Some(s)) = (class_of(t, pat), class_of(t, scrut)) else { return false };
    (t.u.derives_from(p, s) || t.u.derives_from(s, p)) && constrain_simple(t, pat, scrut, (p, s))
}

/// Compares the arguments of the base types of `pat` and `scrut` at their common base class
/// argument by argument, lower bound of one below the upper of the other, those at variant
/// parameters only for a pattern that refines invariantly; all or nothing (dotty's
/// `constrainSimplePatternType`).
fn constrain_simple(t: &mut Typer, pat: TypeId, scrut: TypeId, (p, s): (SymbolId, SymbolId)) -> bool {
    let base = if t.u.derives_from(p, s) { s } else { p };
    let (Some(pt), Some(tp)) = (t.u.base_type(pat, base), t.u.base_type(scrut, base)) else { return false };
    let (Some((_, args_p)), Some((_, args_s))) =
        (t.u.applied(pt).map(|(c, a)| (c, a.to_vec())), t.u.applied(tp).map(|(c, a)| (c, a.to_vec())))
    else {
        return false;
    };
    let invariant = refinement_is_invariant(t, pat);
    let params = t.u.type_params(base);
    let (mark, gadt_mark) = (t.constraint.mark(), t.u.gadt().mark());
    let ok = params.iter().zip(args_s.iter().zip(&args_p)).all(|(&param, (&arg_s, &arg_p))| {
        let variance = t.u.symbol(param).variance();
        let same_class = p == s && t.u.sym_of(arg_p).is_some_and(|b| t.u.is_pattern_bound(b));
        if variance != 0 && !invariant && !same_class {
            return true;
        }
        let ((lo_s, hi_s), (lo_p, hi_p)) = (arg_bounds(t, arg_s), arg_bounds(t, arg_p));
        (variance >= 1 || is_sub(t, lo_s, hi_p)) && (variance <= -1 || is_sub(t, lo_p, hi_s))
    });
    if !ok {
        t.constraint.rollback(mark);
        t.u.gadt_mut().rollback(gadt_mark);
    }
    ok
}

/// The bounds of type argument `a`: its own if it is a wildcard, else `a` twice.
fn arg_bounds(t: &mut Typer, a: TypeId) -> (TypeId, TypeId) {
    match *t.u.ty(a) {
        Type::Bounds(lo, hi) => (lo, hi),
        Type::Wildcard(None) => (tasty::NOTHING, tasty::ANY),
        Type::Wildcard(Some(b)) | Type::Annotated(b, _) => arg_bounds(t, b),
        _ => (a, a),
    }
}

/// Whether a value of `pat`'s type that has a base type has it with the arguments `pat` gives:
/// a singleton, or a final or case class (dotty's `refinementIsInvariant`).
fn refinement_is_invariant(t: &mut Typer, pat: TypeId) -> bool {
    if t.is_singleton(pat) {
        return true;
    }
    if matches!(t.u.ty(pat), Type::And(..) | Type::Or(..)) {
        return false;
    }
    class_of(t, pat).is_some_and(|c| t.u.symbol(c).is_final() || t.u.symbol(c).is_case())
}
