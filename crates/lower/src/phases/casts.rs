//! Erasure's casts: where a reference's erased class does not conform to the one expected, and
//! the class a call's receiver is adapted to.

use super::adapt::from_start;
use crate::Lower;
use crate::definitions::type_ref;
use crate::symbols::{has, is_class};
use tasty::tags::{PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified};
use tasty::{Erased, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// `t.asInstanceOf[T]` for the type `T` erasing to `to`.
pub fn cast(l: &mut Lower, t: TNodeId, to: &Erased) -> TNodeId {
    let span = from_start(l.tpd.span(t));
    let target = type_of(l, to);
    let fun = l.select(t, l.defs.as_instance_of, span);
    let tpt = l.type_tree(target, span);
    let args = l.tpd.list(&[tpt]);
    l.tpd.node(Tree::TypeApply { fun, args }, span, Some(target), None)
}

/// A type erasing to `e`.
fn type_of(l: &mut Lower, e: &Erased) -> TypeId {
    match e {
        Erased::Array(elem) => {
            let elem = type_of(l, elem);
            let array = l.u.class_ref("scala.Array");
            l.u.intern_type(Type::Applied(array, vec![elem]))
        }
        Erased::Prim(c) => {
            let p = l.defs.primitive(*c).expect("a primitive letter").module;
            let class = l.u.module_class(p).expect("a primitive's object class");
            let name = l.u.full_name(class);
            l.u.class_ref(name.trim_end_matches('$'))
        }
        Erased::Class(name) => l.u.class_ref(name),
        _ => l.u.class_ref("java.lang.Object"),
    }
}

/// Whether a value of erased type `from` is a value of erased reference type `to`.
pub fn conforms(l: &mut Lower, from: &Erased, to: &Erased) -> bool {
    match (from, to) {
        (_, Erased::Class(o)) if o == "java.lang.Object" => true,
        _ if from == to => true,
        (Erased::Array(x), Erased::Array(y)) => {
            matches!(**y, Erased::Class(ref o) if o == "java.lang.Object")
                && matches!(**x, Erased::Class(_) | Erased::Array(_))
        }
        (Erased::Class(f), Erased::Class(t)) => {
            let (f, t) = (l.u.class_ref(f), l.u.class_ref(t));
            match (l.u.class_symbol(f), l.u.class_symbol(t)) {
                (Some(f), Some(t)) => l.u.derives_from(f, t),
                _ => false,
            }
        }
        _ => false,
    }
}

/// Whether `t` is a cast `x.asInstanceOf[T]`.
pub fn is_cast(l: &Lower, t: TNodeId) -> bool {
    matches!(*l.tpd.tree(t), Tree::TypeApply { fun, .. } if l.tpd.sym(fun) == Some(l.defs.as_instance_of))
}

/// The erased type the receiver `qualifier` of selection `sel` is adapted to: the class owning
/// the selected member, or the receiver's own type if that is a JDK class that is not public
/// (dotty's `isJvmAccessible`); `None` for a `super` receiver, a member of `Array` or a member
/// outside classes.
pub fn receiver_type(l: &mut Lower, sel: TNodeId, qualifier: TNodeId) -> Option<Erased> {
    if matches!(l.tpd.tree(qualifier), Tree::Super { .. }) {
        return None;
    }
    let owner = l.u.symbol(l.tpd.sym(sel)?).owner.filter(|&o| is_class(l.u, o))?;
    let name = l.u.full_name(owner);
    if name == "scala.Array" {
        return None;
    }
    let jdk = name.starts_with("java.") || name.starts_with("javax.");
    let hidden =
        [PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified].iter().any(|&f| has(l.u, owner, f));
    let tpe = if jdk && hidden { l.u.widen(l.tpd.tpe(qualifier)) } else { type_ref(l.u, owner) };
    Some(l.u.erase(tpe))
}
