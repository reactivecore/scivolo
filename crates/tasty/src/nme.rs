// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/StdNames.scala
//! The well-known member names (dotty's `nme`), spelled as dotty spells them.

/// The name of a class's constructor.
pub const CONSTRUCTOR: &str = "<init>";
/// The name of a class's static initializer.
pub const STATIC_CONSTRUCTOR: &str = "<clinit>";
/// The name of a trait's initializer.
pub const TRAIT_CONSTRUCTOR: &str = "$init$";
/// The name of an anonymous class.
pub const ANON_CLASS: &str = "$anon";
/// The name of the method of a closure.
pub const ANON_FUN: &str = "$anonfun";
/// The name of the factory of an enum's cases.
pub const DOLLAR_NEW: &str = "$new";
/// The name of the field and the constructor parameter of a class's outer instance.
pub const OUTER: &str = "$outer";
/// The name of an extension method's receiver parameter.
pub const SELF: &str = "$this";
pub const EMPTY_PACKAGE: &str = "<empty>";
/// The package of the compiler's own operations, such as `throw`.
pub const OPS_PACKAGE: &str = "<special-ops>";
pub const WILDCARD: &str = "_";
/// The static field of an object's instance.
pub const MODULE_INSTANCE_FIELD: &str = "MODULE$";
/// The parameter of the name of a Java enum's constant.
pub const NAME_DOLLAR: &str = "$name";
/// The constructor parameter of the name of a Java enum's constant.
pub const NAME_DOLLAR_: &str = "_$name";
/// The constructor parameter of the ordinal of a Java enum's constant.
pub const ORDINAL_DOLLAR_: &str = "_$ordinal";

pub const APPLY: &str = "apply";
pub const UNAPPLY: &str = "unapply";
pub const UNAPPLY_SEQ: &str = "unapplySeq";
pub const CLASS_OF: &str = "classOf";
pub const HASH_CODE: &str = "hashCode";
pub const EQUALS: &str = "equals";
pub const TO_STRING: &str = "toString";
pub const CLONE: &str = "clone";
pub const GET_CLASS: &str = "getClass";
pub const AS_INSTANCE_OF: &str = "asInstanceOf";
pub const IS_INSTANCE_OF: &str = "isInstanceOf";
pub const SYNCHRONIZED: &str = "synchronized";
pub const BOX: &str = "box";
pub const UNBOX: &str = "unbox";
pub const MAP: &str = "map";
pub const OF_DIM: &str = "ofDim";
pub const LENGTH_COMPARE: &str = "lengthCompare";
pub const THROW: &str = "throw";

pub const HASHHASH: &str = "##";
pub const ADD: &str = "+";
pub const EQ: &str = "==";
pub const ZAND: &str = "&&";
pub const ZOR: &str = "||";
