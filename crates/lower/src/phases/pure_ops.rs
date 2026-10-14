//! dotty's `isKnownPureOp`: the operations known not to observe their arguments.

use crate::Lower;
use tasty::{PRIMITIVE_VALUE_CLASSES, SymbolId};

const ANY_METHODS: [&str; 9] =
    ["==", "!=", "equals", "hashCode", "toString", "##", "getClass", "isInstanceOf", "$isInstanceOf"];

/// Whether `m` is a method of a primitive class or `String`, or one of dotty's `pureMethods`.
pub fn is_known_pure_op(l: &Lower, m: SymbolId) -> bool {
    let Some(owner) = l.u.symbol(m).owner else { return false };
    let name = l.u.symbol(m).name.as_str();
    let full = l.u.full_name(owner);
    PRIMITIVE_VALUE_CLASSES.contains(&full.as_str())
        || full == "java.lang.String"
        || owner == l.defs.any && ANY_METHODS.contains(&name)
        || owner == l.defs.object && matches!(name, "eq" | "ne")
}
