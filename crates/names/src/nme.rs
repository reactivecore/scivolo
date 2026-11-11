// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/StdNames.scala
//! The well-known names (dotty's `nme` and `tpnme`), spelled as dotty spells them.

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
/// The name of the array of an enum's singleton cases.
pub const DOLLAR_VALUES: &str = "$values";
/// The name of the field and the constructor parameter of a class's outer instance.
pub const OUTER: &str = "$outer";
/// The name of an extension method's receiver parameter.
pub const SELF: &str = "$this";
/// The name of the root package.
pub const ROOT: &str = "<root>";
/// The name `_root_` of the root package in source.
pub const ROOT_PKG: &str = "_root_";
/// The name of the package of definitions outside any package.
pub const EMPTY_PACKAGE: &str = "<empty>";
/// The package of the compiler's own operations, such as `throw`.
pub const OPS_PACKAGE: &str = "<special-ops>";
/// The name of an erroneous tree or symbol.
pub const ERROR: &str = "<error>";
/// The name of no symbol.
pub const NO_NAME: &str = "<none>";
/// The name of the class of repeated parameters.
pub const REPEATED_PARAM_CLASS: &str = "<repeated>";
/// The name `_` of a wildcard.
pub const WILDCARD: &str = "_";
/// The name `_*` of a sequence wildcard.
pub const WILDCARD_STAR: &str = "_*";
/// The static field of an object's instance.
pub const MODULE_INSTANCE_FIELD: &str = "MODULE$";
/// The parameter of the name of a Java enum's constant.
pub const NAME_DOLLAR: &str = "$name";
/// The constructor parameter of the name of a Java enum's constant.
pub const NAME_DOLLAR_: &str = "_$name";
/// The constructor parameter of the ordinal of a Java enum's constant.
pub const ORDINAL_DOLLAR_: &str = "_$ordinal";
/// The name of the first synthetic parameter.
pub const X_0: &str = "x$0";
/// The name of the second synthetic parameter.
pub const X_1: &str = "x$1";
/// The name of the exception a `catch` binds by default.
pub const DEFAULT_EXCEPTION_NAME: &str = "ex$";
/// The name `$asInstanceOf$` of the cast after erasure.
pub const AS_INSTANCE_OF_PM: &str = "$asInstanceOf$";
/// The name `$isInstanceOf$` of the type test after erasure.
pub const IS_INSTANCE_OF_PM: &str = "$isInstanceOf$";
/// The name of the XML scope of a literal.
pub const SCOPE: &str = "$scope";

/// The name of the first element of a product.
pub const SELECTOR_1: &str = "_1";
/// The name of the second element of a product.
pub const SELECTOR_2: &str = "_2";

/// The name `scala`.
pub const SCALA: &str = "scala";
/// The name `java`.
pub const JAVA: &str = "java";
/// The name `lang`.
pub const LANG: &str = "lang";
/// The name `annotation`.
pub const ANNOTATION: &str = "annotation";
/// The name `runtime`.
pub const RUNTIME: &str = "runtime";
/// The name `caps`.
pub const CAPS: &str = "caps";
/// The name `internal`.
pub const INTERNAL: &str = "internal";
/// The name `language`.
pub const LANGUAGE: &str = "language";
/// The name `EnumValue`.
pub const ENUM_VALUE: &str = "EnumValue";
/// The name `retains`.
pub const RETAINS: &str = "retains";
/// The name `retainsCap`.
pub const RETAINS_CAP: &str = "retainsCap";
/// The name `retainsByName`.
pub const RETAINS_BY_NAME: &str = "retainsByName";

/// The language feature `implicitConversions`.
pub const IMPLICIT_CONVERSIONS: &str = "implicitConversions";
/// The language feature `higherKinds`.
pub const HIGHER_KINDS: &str = "higherKinds";
/// The language feature `postfixOps`.
pub const POSTFIX_OPS: &str = "postfixOps";
/// The language feature `strictEquality`.
pub const STRICT_EQUALITY: &str = "strictEquality";
/// The language feature `dynamics`.
pub const DYNAMICS: &str = "dynamics";
/// The language feature `noAutoTupling`.
pub const NO_AUTO_TUPLING: &str = "noAutoTupling";
/// The language feature `unsafeNulls`.
pub const UNSAFE_NULLS: &str = "unsafeNulls";
/// The language feature `captureChecking`.
pub const CAPTURE_CHECKING: &str = "captureChecking";

/// The name `apply`.
pub const APPLY: &str = "apply";
/// The name `unapply`.
pub const UNAPPLY: &str = "unapply";
/// The name `unapplySeq`.
pub const UNAPPLY_SEQ: &str = "unapplySeq";
/// The name `update`.
pub const UPDATE: &str = "update";
/// The name `copy`.
pub const COPY: &str = "copy";
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
/// The name `finalize`.
pub const FINALIZE: &str = "finalize";
/// The name `getClass`.
pub const GET_CLASS: &str = "getClass";
/// The name `asInstanceOf`.
pub const AS_INSTANCE_OF: &str = "asInstanceOf";
/// The name `isInstanceOf`.
pub const IS_INSTANCE_OF: &str = "isInstanceOf";
/// The name `synchronized`.
pub const SYNCHRONIZED: &str = "synchronized";
/// The name `eq` of reference equality.
pub const REF_EQ: &str = "eq";
/// The name `ne` of reference inequality.
pub const REF_NE: &str = "ne";
/// The name `canEqual`.
pub const CAN_EQUAL: &str = "canEqual";
/// The name `canEqualAny`.
pub const CAN_EQUAL_ANY: &str = "canEqualAny";
/// The name `productArity`.
pub const PRODUCT_ARITY: &str = "productArity";
/// The name `productPrefix`.
pub const PRODUCT_PREFIX: &str = "productPrefix";
/// The name `productElement`.
pub const PRODUCT_ELEMENT: &str = "productElement";
/// The name `productElementName`.
pub const PRODUCT_ELEMENT_NAME: &str = "productElementName";
/// The name `productIterator`.
pub const PRODUCT_ITERATOR: &str = "productIterator";
/// The name `fromProduct`.
pub const FROM_PRODUCT: &str = "fromProduct";
/// The name `ordinal`.
pub const ORDINAL: &str = "ordinal";
/// The name `fromOrdinal`.
pub const FROM_ORDINAL: &str = "fromOrdinal";
/// The name `values`.
pub const VALUES: &str = "values";
/// The name `valueOf`.
pub const VALUE_OF: &str = "valueOf";
/// The name `name`.
pub const NAME: &str = "name";
/// The name `value`.
pub const VALUE: &str = "value";
/// The name `readResolve`.
pub const READ_RESOLVE: &str = "readResolve";
/// The name `writeReplace`.
pub const WRITE_REPLACE: &str = "writeReplace";
/// The name `derived`.
pub const DERIVED: &str = "derived";
/// The name `deferred`.
pub const DEFERRED: &str = "deferred";
/// The name `main`.
pub const MAIN: &str = "main";
/// The name `args`.
pub const ARGS: &str = "args";
/// The name `error`.
pub const ERROR_METHOD: &str = "error";

/// The name `map`.
pub const MAP: &str = "map";
/// The name `flatMap`.
pub const FLAT_MAP: &str = "flatMap";
/// The name `withFilter`.
pub const WITH_FILTER: &str = "withFilter";
/// The name `foreach`.
pub const FOREACH: &str = "foreach";
/// The name `isEmpty`.
pub const IS_EMPTY: &str = "isEmpty";
/// The name `get`.
pub const GET: &str = "get";
/// The name `length`.
pub const LENGTH: &str = "length";
/// The name `lengthCompare`.
pub const LENGTH_COMPARE: &str = "lengthCompare";
/// The name `drop`.
pub const DROP: &str = "drop";
/// The name `dropRight`.
pub const DROP_RIGHT: &str = "dropRight";
/// The name `takeRight`.
pub const TAKE_RIGHT: &str = "takeRight";
/// The name `tail`.
pub const TAIL: &str = "tail";
/// The name `toSeq`.
pub const TO_SEQ: &str = "toSeq";
/// The name `toArray`.
pub const TO_ARRAY: &str = "toArray";
/// The name `toTuple`.
pub const TO_TUPLE: &str = "toTuple";
/// The name `hasNext`.
pub const HAS_NEXT: &str = "hasNext";
/// The name `next`.
pub const NEXT: &str = "next";
/// The name `isDefinedAt`.
pub const IS_DEFINED_AT: &str = "isDefinedAt";
/// The name `applyOrElse`.
pub const APPLY_OR_ELSE: &str = "applyOrElse";
/// The name `append`.
pub const APPEND: &str = "append";
/// The name `add`.
pub const ADD_METHOD: &str = "add";
/// The name `elem`.
pub const ELEM: &str = "elem";
/// The name `create`.
pub const CREATE: &str = "create";
/// The name `wrap`.
pub const WRAP: &str = "wrap";
/// The name `identity`.
pub const IDENTITY: &str = "identity";
/// The name `spread`.
pub const SPREAD: &str = "spread";

/// The name `selectDynamic`.
pub const SELECT_DYNAMIC: &str = "selectDynamic";
/// The name `applyDynamic`.
pub const APPLY_DYNAMIC: &str = "applyDynamic";
/// The name `applyDynamicNamed`.
pub const APPLY_DYNAMIC_NAMED: &str = "applyDynamicNamed";
/// The name `updateDynamic`.
pub const UPDATE_DYNAMIC: &str = "updateDynamic";

/// The name `toByte`.
pub const TO_BYTE: &str = "toByte";
/// The name `toShort`.
pub const TO_SHORT: &str = "toShort";
/// The name `toChar`.
pub const TO_CHAR: &str = "toChar";
/// The name `toInt`.
pub const TO_INT: &str = "toInt";
/// The name `toLong`.
pub const TO_LONG: &str = "toLong";
/// The name `toFloat`.
pub const TO_FLOAT: &str = "toFloat";
/// The name `toDouble`.
pub const TO_DOUBLE: &str = "toDouble";

/// The name `box`.
pub const BOX: &str = "box";
/// The name `unbox`.
pub const UNBOX: &str = "unbox";
/// The name `ofDim`.
pub const OF_DIM: &str = "ofDim";
/// The name `throw`.
pub const THROW: &str = "throw";
/// The name `newArray` of `scala.runtime.Arrays`' array factory.
pub const NEW_ARRAY: &str = "newArray";
/// The name `objCAS` of Scala Native's compare-and-set of a lazy value's field.
pub const OBJ_CAS: &str = "objCAS";
/// The name `genericWrapArray`.
pub const GENERIC_WRAP_ARRAY: &str = "genericWrapArray";
/// The name `wrapRefArray`.
pub const WRAP_REF_ARRAY: &str = "wrapRefArray";
/// The name `seqToArray`.
pub const SEQ_TO_ARRAY: &str = "seqToArray";
/// The name `toObjectArray`.
pub const TO_OBJECT_ARRAY: &str = "toObjectArray";
/// The name `isArray`.
pub const IS_ARRAY: &str = "isArray";
/// The name `anyHash`.
pub const ANY_HASH: &str = "anyHash";
/// The name `longHash`.
pub const LONG_HASH: &str = "longHash";
/// The name `doubleHash`.
pub const DOUBLE_HASH: &str = "doubleHash";
/// The name `floatHash`.
pub const FLOAT_HASH: &str = "floatHash";
/// The name `releaseFence`.
pub const RELEASE_FENCE: &str = "releaseFence";

/// The name `##`.
pub const HASHHASH: &str = "##";
/// The name `+`.
pub const ADD: &str = "+";
/// The name `-`.
pub const MINUS: &str = "-";
/// The name `*`.
pub const MUL: &str = "*";
/// The name `/`.
pub const DIV: &str = "/";
/// The name `%`.
pub const MOD: &str = "%";
/// The name `&`.
pub const AND: &str = "&";
/// The name `|`.
pub const OR: &str = "|";
/// The name `^`.
pub const XOR: &str = "^";
/// The name `<<`.
pub const LSL: &str = "<<";
/// The name `>>>`.
pub const LSR: &str = ">>>";
/// The name `>>`.
pub const ASR: &str = ">>";
/// The name `==`.
pub const EQ: &str = "==";
/// The name `!=`.
pub const NE: &str = "!=";
/// The name `<`.
pub const LT: &str = "<";
/// The name `<=`.
pub const LE: &str = "<=";
/// The name `>`.
pub const GT: &str = ">";
/// The name `>=`.
pub const GE: &str = ">=";
/// The name `&&`.
pub const ZAND: &str = "&&";
/// The name `||`.
pub const ZOR: &str = "||";
/// The prefix `unary_` of a prefix operator's name.
pub const UNARY_PREFIX: &str = "unary_";
/// The name `unary_!`.
pub const UNARY_NOT: &str = "unary_!";
/// The name `unary_~`.
pub const UNARY_TILDE: &str = "unary_~";
/// The name `unary_+`.
pub const UNARY_PLUS: &str = "unary_+";
/// The name `unary_-`.
pub const UNARY_MINUS: &str = "unary_-";
/// The name `!`.
pub const NOT: &str = "!";
/// The name `zero`.
pub const ZERO: &str = "zero";
/// The name `TYPE`.
pub const TYPE_: &str = "TYPE";
/// The suffix `$package` of a source file's package object.
pub const TOPLEVEL_SUFFIX: &str = "$package";
/// The prefix `rs$line$` of a REPL line's wrapper object.
pub const REPL_SESSION_LINE: &str = "rs$line$";
