//! dotty's `SpecializeTuples`, which makes `Tuple1` and `Tuple2` of primitive types instances of
//! their specialized classes, is not ported: refuses `getClass` of such tuples, which tells the
//! classes apart.

use super::walk::subtrees;
use crate::{Lower, Result, unsupported};
use tasty::{Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The element classes of the specialized `Tuple1` and `Tuple2` classes.
const TUPLE1: [&str; 3] = ["scala.Int", "scala.Long", "scala.Double"];
const TUPLE2: [&str; 5] = ["scala.Int", "scala.Long", "scala.Double", "scala.Char", "scala.Boolean"];

/// Refuses `getClass` of a specializable tuple in class definition `td`.
pub fn check(l: &mut Lower, td: TNodeId) -> Result<()> {
    for t in subtrees(l.tpd, td) {
        let Tree::Apply { fun, .. } = *l.tpd.tree(t) else { continue };
        let Tree::TypeApply { fun: select, .. } = *l.tpd.tree(fun) else { continue };
        let Tree::Select { qualifier, .. } = *l.tpd.tree(select) else { continue };
        let get_class = l.tpd.sym(select).is_some_and(|s| {
            let owner = l.u.symbol(s).owner;
            l.u.symbol(s).name == "getClass" && owner == Some(l.u.builtins().any)
        });
        let receiver = l.tpd.tpe(qualifier);
        if get_class && is_specializable_tuple(l, receiver) {
            return unsupported(l.tpd.span(t), "getClass of specialized tuples");
        }
    }
    Ok(())
}

/// Whether `t` is a `Tuple1` or `Tuple2` of element types dotty specializes (dotty's
/// `isSpecializableTuple`).
fn is_specializable_tuple(l: &mut Lower, t: TypeId) -> bool {
    let t = l.u.widen(t);
    let t = l.u.dealias(t);
    let Type::Applied(tycon, args) = l.u.types().get(t).clone() else { return false };
    let Some(class) = l.u.class_symbol(tycon) else { return false };
    let elems: &[&str] = match l.u.full_name(class).as_str() {
        "scala.Tuple1" => &TUPLE1,
        "scala.Tuple2" => &TUPLE2,
        _ => return false,
    };
    args.iter().all(|&a| l.u.class_symbol(a).is_some_and(|c| elems.contains(&l.u.full_name(c).as_str())))
}
