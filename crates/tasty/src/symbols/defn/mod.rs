// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//! The well-known symbols (dotty's `defn`) and the full names of the library symbols referred to
//! by name.

mod table;

pub use table::Defn;

/// The largest arity of a `TupleN` class (dotty's `MaxTupleArity`).
pub const MAX_TUPLE_ARITY: usize = 22;

pub const SCALA_ANNOTATION_INTERNAL: &str = "scala.annotation.internal";

pub const OBJECT: &str = "java.lang.Object";
pub const STRING: &str = "java.lang.String";
pub const CLASS: &str = "java.lang.Class";
pub const THROWABLE: &str = "java.lang.Throwable";
pub const JAVA_ENUM: &str = "java.lang.Enum";
pub const FUNCTIONAL_INTERFACE: &str = "java.lang.FunctionalInterface";

pub const ANY_VAL: &str = "scala.AnyVal";
pub const UNIT: &str = "scala.Unit";
pub const BOOLEAN: &str = "scala.Boolean";
pub const INT: &str = "scala.Int";
pub const BOXED_UNIT: &str = "scala.runtime.BoxedUnit";
pub const ARRAY: &str = "scala.Array";
/// dotty's `<repeated>`, the class of the types of repeated parameters.
pub const REPEATED: &str = "scala.<repeated>";
pub const SOME: &str = "scala.Some";
pub const DYNAMIC: &str = "scala.Dynamic";
pub const SELECTABLE: &str = "scala.Selectable";
pub const FUNCTION0: &str = "scala.Function0";
/// The class of functions above the library's arities, applied to an array of their arguments.
pub const FUNCTION_XXL: &str = "scala.runtime.FunctionXXL";
pub const PARTIAL_FUNCTION: &str = "scala.PartialFunction";
pub const POLY_FUNCTION: &str = "scala.PolyFunction";
pub const CONTEXT_FUNCTION1: &str = "scala.ContextFunction1";
/// The intersection type constructor.
pub const AND: &str = "scala.&";
pub const THROWS: &str = "scala.throws";
/// The class of the `Predef` object.
pub const PREDEF_CLASS: &str = "scala.Predef$";
pub const CLASS_OF: &str = "scala.Predef$.classOf";
pub const REFLECT_PACKAGE_CLASS: &str = "scala.reflect.package$";
pub const REFLECTIVE_SELECTABLE: &str = "scala.reflect.Selectable";
pub const SUMMON_FROM: &str = "scala.compiletime.package$package$.summonFrom";

/// The class of tuples, and the prefix of the names of `TupleN`.
pub const TUPLE: &str = "scala.Tuple";
/// The class of non-empty tuples, `*:`.
pub const PAIR: &str = "scala.*:";
/// The class of the `EmptyTuple` object.
pub const EMPTY_TUPLE_CLASS: &str = "scala.EmptyTuple$";
/// The `EmptyTuple` object.
pub const EMPTY_TUPLE: &str = "scala.Tuple$package$.EmptyTuple";
pub const TUPLES_CLASS: &str = "scala.runtime.Tuples$";
/// The class of the `NamedTuple` object.
pub const NAMED_TUPLE_CLASS: &str = "scala.NamedTuple$";
pub const NAMED_TUPLE: &str = "scala.NamedTuple$.NamedTuple";
pub const IMMUTABLE_SEQ: &str = "scala.collection.immutable.Seq";
/// The class of case classes and tuples, whose selectors `_1`, ..., `_n` patterns match.
pub const PRODUCT: &str = "scala.Product";
pub const EQUALS: &str = "scala.Equals";

/// The type class of equality evidence.
pub const CAN_EQUAL: &str = "scala.CanEqual";
pub const CLASS_TAG: &str = "scala.reflect.ClassTag";
pub const TYPE_TEST: &str = "scala.reflect.TypeTest";
pub const MANIFEST: &str = "scala.reflect.Manifest";
pub const OPT_MANIFEST: &str = "scala.reflect.OptManifest";
pub const VALUE_OF: &str = "scala.ValueOf";
pub const TUPLED_FUNCTION: &str = "scala.util.TupledFunction";
pub const MIRROR: &str = "scala.deriving.Mirror";
pub const MIRROR_PRODUCT: &str = "scala.deriving.Mirror$.Product";
pub const MIRROR_SUM: &str = "scala.deriving.Mirror$.Sum";

pub const QUOTED_EXPR: &str = "scala.quoted.Expr";
pub const QUOTES: &str = "scala.quoted.Quotes";
pub const TYPE_OF: &str = "scala.quoted.Type$.of";
/// The class of the `scala.quoted.runtime.Expr` object.
pub const RUNTIME_EXPR: &str = "scala.quoted.runtime.Expr$";
/// The class of the `scala.quoted.runtime.Patterns` object.
pub const PATTERNS: &str = "scala.quoted.runtime.Patterns$";

pub const TAILREC: &str = "scala.annotation.tailrec";
pub const STATIC_ANNOT: &str = "scala.annotation.static";
pub const VARARGS: &str = "scala.annotation.varargs";
pub const UNROLL: &str = "scala.annotation.unroll";
pub const EXPERIMENTAL: &str = "scala.annotation.experimental";
pub const STABLE_NULL: &str = "scala.annotation.stableNull";
pub const UNUSED: &str = "scala.annotation.unused";
pub const UNCHECKED_VARIANCE: &str = "scala.annotation.unchecked.uncheckedVariance";
/// The annotation of the types of repeated parameters in TASTy.
pub const REPEATED_ANNOT: &str = "scala.annotation.internal.Repeated";
/// The annotation of the types of `inline` parameters.
pub const INLINE_PARAM: &str = "scala.annotation.internal.InlineParam";
pub const ERASED_PARAM: &str = "scala.annotation.internal.ErasedParam";
pub const SOURCE_FILE: &str = "scala.annotation.internal.SourceFile";
pub const CHILD: &str = "scala.annotation.internal.Child";
pub const PURE: &str = "scala.caps.Pure";
pub const INFERRED: &str = "scala.caps.internal$.inferred";
