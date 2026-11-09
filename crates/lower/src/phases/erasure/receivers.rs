// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala

use crate::Lower;
use crate::definitions::type_ref;
use crate::phases::select_static::is_scala_static;
use crate::symbols::has;
use tasty::Erased;
use tasty::tags::{PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified};
use typer::tpd::{TNodeId, Tree};

/// The erased type the receiver `qualifier` of selection `sel` is adapted to: the class owning
/// the selected member, or the receiver's own type if that is a JDK class that is not public
/// (dotty's `isJvmAccessible`); `None` for a `super` receiver, an `@static` member, a member of
/// `Array` or a member outside classes.
pub fn receiver_type(l: &mut Lower, sel: TNodeId, qualifier: TNodeId) -> Option<Erased> {
    if matches!(l.tpd.tree(qualifier), Tree::Super { .. }) {
        return None;
    }
    let member = l.tpd.sym(sel).filter(|&m| !is_scala_static(l, m))?;
    let owner = l.u.symbol(member).owner.filter(|&o| l.u.is_class(o))?;
    let name = l.u.full_name(owner);
    if name == "scala.Array" {
        let array = l.u.widen(l.tpd.tpe(qualifier));
        let e = l.u.erase(array);
        return matches!(e, Erased::Array(_)).then_some(e);
    }
    let jdk = l.u.may_be_java(owner) && (name.starts_with("java.") || name.starts_with("javax."));
    let hidden =
        [PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified].iter().any(|&f| has(l.u, owner, f));
    let tpe = if jdk && hidden { l.u.widen(l.tpd.tpe(qualifier)) } else { type_ref(l.u, owner) };
    Some(l.u.erase_boxed(tpe))
}
