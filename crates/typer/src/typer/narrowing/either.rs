// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//! One of two pattern constraints, keeping only what both need (dotty's `necessaryEither`).

use super::super::Typer;
use super::constrain::{subsumes, var_bounds};
use tasty::{SymbolId, TypeId, TypeVars};

/// Whether `first` holds, or else `second`; when both do, the constraints and GADT constraint
/// of the one the other subsumes, else those before both (dotty's `necessaryEither`).
pub fn either(
    t: &mut Typer,
    first: impl FnOnce(&mut Typer) -> bool,
    second: impl FnOnce(&mut Typer) -> bool,
) -> bool {
    let (mark, gadt_mark) = (t.constraint.mark(), t.u.gadt().mark());
    if !first(t) {
        return second(t);
    }
    let (left, left_gadt) = (t.constraint.changes_since(mark), t.u.gadt().changes_since(gadt_mark));
    let left_bounds = var_bounds(&t.constraint, &left.vars);
    t.constraint.rollback(mark);
    t.u.gadt_mut().rollback(gadt_mark);
    let pre: Vec<SymbolId> = t.u.gadt().symbols().collect();
    if !second(t) {
        t.constraint.rollback(mark);
        t.u.gadt_mut().rollback(gadt_mark);
        t.constraint.redo(left);
        if let Some(g) = left_gadt {
            t.u.gadt_mut().redo(g);
        }
        return true;
    }

    let right = t.constraint.changes_since(mark);
    let right_gadt = t.u.gadt().changes_since(gadt_mark);
    let mut vars = left.vars.clone();
    vars.extend(right.vars.iter().filter(|v| !left.vars.contains(v)));
    let right_bounds = var_bounds(&t.constraint, &vars);
    t.constraint.rollback(mark);
    t.u.gadt_mut().rollback(gadt_mark);
    let pre_bounds = var_bounds(&t.constraint, &vars);
    let left_bounds: Vec<(TypeId, TypeId)> = vars
        .iter()
        .zip(&pre_bounds)
        .map(|(v, &pre)| left.vars.iter().position(|w| w == v).map_or(pre, |i| left_bounds[i]))
        .collect();

    let keep_right = subsumes(t.u, &left_bounds, &right_bounds, &pre_bounds)
        && t.u.gadt_subsumes(left_gadt.as_ref(), right_gadt.as_ref(), &pre);
    let keep_left = !keep_right
        && subsumes(t.u, &right_bounds, &left_bounds, &pre_bounds)
        && t.u.gadt_subsumes(right_gadt.as_ref(), left_gadt.as_ref(), &pre);
    if keep_right {
        t.constraint.redo(right);
        if let Some(g) = right_gadt {
            t.u.gadt_mut().redo(g);
        }
    } else if keep_left {
        t.constraint.redo(left);
        if let Some(g) = left_gadt {
            t.u.gadt_mut().redo(g);
        }
    }
    true
}
