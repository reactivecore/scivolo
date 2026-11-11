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
/// The name of the package of definitions outside any package.
pub const EMPTY_PACKAGE: &str = "<empty>";
/// The package of the compiler's own operations, such as `throw`.
pub const OPS_PACKAGE: &str = "<special-ops>";
/// The name `_` of a wildcard.
pub const WILDCARD: &str = "_";
/// The static field of an object's instance.
pub const MODULE_INSTANCE_FIELD: &str = "MODULE$";
/// The parameter of the name of a Java enum's constant.
pub const NAME_DOLLAR: &str = "$name";
/// The constructor parameter of the name of a Java enum's constant.
pub const NAME_DOLLAR_: &str = "_$name";
/// The constructor parameter of the ordinal of a Java enum's constant.
pub const ORDINAL_DOLLAR_: &str = "_$ordinal";

/// The name `apply`.
pub const APPLY: &str = "apply";
/// The name `unapply`.
pub const UNAPPLY: &str = "unapply";
/// The name `unapplySeq`.
pub const UNAPPLY_SEQ: &str = "unapplySeq";
/// The name `classOf`.
pub const CLASS_OF: &str = "classOf";
/// The name `hashCode`.
pub const HASH_CODE: &str = "hashCode";
/// The name `equals`.
pub const EQUALS: &str = "equals";
/// The name `toString`.
pub const TO_STRING: &str = "toString";
/// The name `clone`.
pub const CLONE: &str = "clone";
/// The name `getClass`.
pub const GET_CLASS: &str = "getClass";
/// The name `asInstanceOf`.
pub const AS_INSTANCE_OF: &str = "asInstanceOf";
/// The name `isInstanceOf`.
pub const IS_INSTANCE_OF: &str = "isInstanceOf";
/// The name `synchronized`.
pub const SYNCHRONIZED: &str = "synchronized";
/// The name `box`.
pub const BOX: &str = "box";
/// The name `unbox`.
pub const UNBOX: &str = "unbox";
/// The name `map`.
pub const MAP: &str = "map";
/// The name `ofDim`.
pub const OF_DIM: &str = "ofDim";
/// The name `lengthCompare`.
pub const LENGTH_COMPARE: &str = "lengthCompare";
/// The name `throw`.
pub const THROW: &str = "throw";
/// The name `newArray` of `scala.runtime.Arrays`' array factory.
pub const NEW_ARRAY: &str = "newArray";
/// The name `objCAS` of Scala Native's compare-and-set of a lazy value's field.
pub const OBJ_CAS: &str = "objCAS";

/// The name `##`.
pub const HASHHASH: &str = "##";
/// The name `+`.
pub const ADD: &str = "+";
/// The name `==`.
pub const EQ: &str = "==";
/// The name `&&`.
pub const ZAND: &str = "&&";
/// The name `||`.
pub const ZOR: &str = "||";
