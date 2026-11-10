// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! An extractor pattern's type constrained by a union selector type (dotty's
//! `constrainPatternType` on an `OrType` scrutinee, `necessaryEither`).

use super::super::Typer;
use crate::constraint::Constraint;
use tasty::{Type, TypeId, TypeVars, Universe, VarId};

/// Tells whether pattern type `pat` conforms to `sel`, constraining type variables; for a union
/// `sel`, by each operand separately, keeping the weaker of two constraints or neither if they
/// are incomparable; a union `pat` that does not conform likewise by each operand.
pub fn constrain_pattern(t: &mut Typer, pat: TypeId, sel: TypeId) -> Result<bool, &'static str> {
    let dealiased = t.dealiased(sel);
    if let Type::Or(a, b, _) = *t.u.types().get(dealiased) {
        return constrain_either(t, (pat, a), (pat, b));
    }
    if t.conforms(pat, sel) {
        return Ok(true);
    }
    let dealiased = t.dealiased(pat);
    match *t.u.types().get(dealiased) {
        Type::Or(a, b, _) => constrain_either(t, (a, sel), (b, sel)),
        _ => Ok(t.types().constrain_upcast(pat, sel)),
    }
}

/// Tells whether one of the pairs of pattern and selector types constrains, keeping the weaker
/// of two constraints or neither if they are incomparable (dotty's `either`).
fn constrain_either(t: &mut Typer, l: (TypeId, TypeId), r: (TypeId, TypeId)) -> Result<bool, &'static str> {
    let mark = t.constraint.mark();
    if !constrain_operand(t, l.0, l.1)? {
        t.constraint.rollback(mark);
        return constrain_operand(t, r.0, r.1);
    }
    let left = t.constraint.changes_since(mark);
    let left_bounds = var_bounds(&t.constraint, &left.vars);
    t.constraint.rollback(mark);
    if !constrain_operand(t, r.0, r.1)? {
        t.constraint.rollback(mark);
        t.constraint.redo(left);
        return Ok(true);
    }
    let right = t.constraint.changes_since(mark);
    let mut vars = left.vars.clone();
    vars.extend(right.vars.iter().filter(|v| !left.vars.contains(v)));
    let right_bounds = var_bounds(&t.constraint, &vars);
    t.constraint.rollback(mark);
    let pre_bounds = var_bounds(&t.constraint, &vars);
    let left_bounds: Vec<(TypeId, TypeId)> = vars
        .iter()
        .zip(&pre_bounds)
        .map(|(v, &pre)| left.vars.iter().position(|w| w == v).map_or(pre, |i| left_bounds[i]))
        .collect();
    if subsumes(t.u, &left_bounds, &right_bounds, &pre_bounds) {
        t.constraint.redo(right);
    } else if subsumes(t.u, &right_bounds, &left_bounds, &pre_bounds) {
        t.constraint.redo(left);
    }
    Ok(true)
}

/// Constrains pattern type `pat` by `sel`, one operand of a union, as `constrain_pattern` does;
/// outside the subset where it fails although the classes of `pat` and `sel` might share an
/// instance.
fn constrain_operand(t: &mut Typer, pat: TypeId, sel: TypeId) -> Result<bool, &'static str> {
    if constrain_pattern(t, pat, sel)? {
        return Ok(true);
    }
    let (p, s) = (t.u.class_symbol(pat), t.u.class_symbol(sel));
    let (Some(p), Some(s)) = (p, s) else { return Err("pattern against an operand of a union") };
    if t.types().classes_may_be_compatible(p, s) {
        return Err("pattern against an operand of a union");
    }
    Ok(false)
}

/// Lists the bounds of `vars`, both an instance if a variable has one.
fn var_bounds(constraint: &Constraint, vars: &[VarId]) -> Vec<(TypeId, TypeId)> {
    vars.iter().map(|&v| constraint.instance(v).map_or_else(|| constraint.bounds(v), |t| (t, t))).collect()
}

/// Tells whether the bounds `c1` lie within the bounds `c2`: always if `c2` is `pre`, never if only
/// `c1` is (dotty's `subsumes`).
fn subsumes(
    u: &mut Universe,
    c1: &[(TypeId, TypeId)],
    c2: &[(TypeId, TypeId)],
    pre: &[(TypeId, TypeId)],
) -> bool {
    if c2 == pre {
        return true;
    }
    if c1 == pre {
        return false;
    }
    c1.iter().zip(c2).all(|(&(lo1, hi1), &(lo2, hi2))| u.is_subtype(lo2, lo1) && u.is_subtype(hi1, hi2))
}
