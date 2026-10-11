//! The test types dotty tests by calls of runtime methods: tuple types (`Tuples.isInstanceOf..`)
//! and arrays of elements that are not of a class (`ScalaRunTime.isArray`).

use crate::Lower;
use tasty::{Kind, Type, TypeId};

/// What a test of dealiased type `tpe` is if it needs a runtime method.
pub fn runtime_test(l: &mut Lower, tpe: TypeId) -> Option<&'static str> {
    let tuple = l.u.class_symbol(tpe).map(|c| l.u.full_name(c));
    if tuple.is_some_and(|n| {
        ["scala.Tuple", "scala.NonEmptyTuple", "scala.*:", "scala.EmptyTuple"].contains(&n.as_str())
    }) {
        return Some("type tests of tuple types");
    }
    is_generic_array(l, tpe).then_some("type tests of generic arrays")
}

/// Whether `tpe` is an array, of arrays, of elements that are not of a class.
fn is_generic_array(l: &mut Lower, tpe: TypeId) -> bool {
    let mut elem = tpe;
    let mut array = false;
    loop {
        let dealiased = l.u.dealias(elem);
        let Type::Applied(tycon, args) = l.u.types().get(dealiased).clone() else { break };
        if l.u.class_symbol(tycon).is_none_or(|c| l.u.full_name(c) != "scala.Array") || args.len() != 1 {
            break;
        }
        (elem, array) = (args[0], true);
    }
    let dealiased = l.u.dealias(elem);
    let class = match *l.u.types().get(dealiased) {
        Type::Sym(_, s) => Some(s),
        Type::Applied(..) | Type::TypeRef(..) => l.u.class_symbol(dealiased),
        _ => None,
    };
    array && class.is_none_or(|c| l.u.symbol(c).kind != Kind::Class)
}
