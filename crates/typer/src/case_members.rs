// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala
//! The members dotty's `PostTyper` synthesizes in case classes (`SyntheticMembers`).

use crate::defn::{EQUALS, PRODUCT};
use names::nme;
use tasty::{SymbolId, Universe};

/// The names of the synthesized members.
const SYNTHESIZED: [&str; 8] = [
    nme::HASH_CODE,
    nme::EQUALS,
    nme::TO_STRING,
    nme::CAN_EQUAL,
    nme::PRODUCT_ARITY,
    nme::PRODUCT_PREFIX,
    nme::PRODUCT_ELEMENT,
    nme::PRODUCT_ELEMENT_NAME,
];

/// The classes whose members of those names the synthesized ones override.
const OVERRIDDEN_OWNERS: [&str; 4] = ["scala.Any", "java.lang.Object", PRODUCT, EQUALS];

/// Whether a case class synthesizes member `name` overriding the one that `owner` declares
/// (dotty's `SyntheticMembers.existingDef`).
pub fn is_synthesized_override(u: &Universe, name: &str, owner: SymbolId) -> bool {
    SYNTHESIZED.contains(&name) && OVERRIDDEN_OWNERS.contains(&u.full_name(owner).as_str())
}
