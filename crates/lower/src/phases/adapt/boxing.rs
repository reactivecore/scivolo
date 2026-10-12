//! Boxing an unboxed box as the box itself (dotty's `safelyRemovableUnboxArg`).

use crate::Lower;
use crate::phases::type_tests::boxed_class;
use typer::tpd::{TNodeId, Tree};

/// The argument of `t` if `t` unboxes a value whose class is a box class.
pub fn removable_unbox_arg(l: &mut Lower, t: TNodeId) -> Option<TNodeId> {
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return None };
    l.defs.unboxed_by(l.tpd.sym(fun)?)?;
    let &[arg] = l.tpd.items(args) else { return None };
    let widened = l.u.widen(l.tpd.tpe(arg));
    let name = l.u.class_symbol(widened).map(|c| l.u.full_name(c))?;
    let boxed = "zcbsijfd".chars().filter_map(boxed_class).any(|b| b == name);
    (boxed || name == "scala.runtime.BoxedUnit").then_some(arg)
}
