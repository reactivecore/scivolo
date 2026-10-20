// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/serialization/Tags.scala
//! Serialization tags from `nir/serialization/Tags.scala` (0.5.12). Bin/Comp/Conv/MemoryOrder
//! tags are `index + 1` into the tables in `nir.rs`.

pub mod attr {
    pub const MAY_INLINE: u8 = 1;
    pub const INLINE_HINT: u8 = 2;
    pub const NO_INLINE: u8 = 3;
    pub const ALWAYS_INLINE: u8 = 4;
    pub const MAY_SPECIALIZE: u8 = 5;
    pub const NO_SPECIALIZE: u8 = 6;
    pub const UN_OPT: u8 = 7;
    pub const NO_OPT: u8 = 8;
    pub const DID_OPT: u8 = 9;
    pub const BAIL_OPT: u8 = 10;
    pub const EXTERN: u8 = 11;
    pub const LINK: u8 = 12;
    pub const DYN: u8 = 13;
    pub const STUB: u8 = 14;
    pub const ABSTRACT: u8 = 15;
    pub const VOLATILE: u8 = 16;
    pub const FINAL: u8 = 17;
    pub const SAFE_PUBLISH: u8 = 18;
    pub const LINKTIME_RESOLVED: u8 = 19;
    pub const USES_INTRINSIC: u8 = 20;
    pub const ALIGN: u8 = 21;
    pub const DEFINE: u8 = 22;
    pub const LINK_CPP_RUNTIME: u8 = 23;
}

pub mod defn {
    pub const VAR: u8 = 1;
    pub const CONST: u8 = 2;
    pub const DECLARE: u8 = 3;
    pub const DEFINE: u8 = 4;
    pub const TRAIT: u8 = 5;
    pub const CLASS: u8 = 6;
    pub const MODULE: u8 = 7;
}

pub mod inst {
    pub const LABEL: u8 = 1;
    pub const LET: u8 = 2;
    pub const LET_UNWIND: u8 = 3;
    pub const RET: u8 = 4;
    pub const JUMP: u8 = 5;
    pub const IF: u8 = 6;
    pub const SWITCH: u8 = 7;
    pub const THROW: u8 = 8;
    pub const UNREACHABLE: u8 = 9;
    pub const LINKTIME_IF: u8 = 10;
}

pub mod global {
    pub const NONE: u8 = 1;
    pub const TOP: u8 = 2;
    pub const MEMBER: u8 = 3;
}

pub mod next {
    pub const NONE: u8 = 1;
    pub const UNWIND: u8 = 2;
    pub const CASE: u8 = 3;
    pub const LABEL: u8 = 4;
}

pub mod op {
    pub const CALL: u8 = 1;
    pub const LOAD: u8 = 2;
    pub const LOAD_ATOMIC: u8 = 3;
    pub const STORE: u8 = 4;
    pub const STORE_ATOMIC: u8 = 5;
    pub const ELEM: u8 = 6;
    pub const EXTRACT: u8 = 7;
    pub const INSERT: u8 = 8;
    pub const STACKALLOC: u8 = 9;
    pub const BIN: u8 = 10;
    pub const COMP: u8 = 11;
    pub const CONV: u8 = 12;
    pub const CLASSALLOC: u8 = 13;
    pub const CLASSALLOC_ZONE: u8 = 14;
    pub const FIELD: u8 = 15;
    pub const FIELDLOAD: u8 = 16;
    pub const FIELDSTORE: u8 = 17;
    pub const METHOD: u8 = 18;
    pub const MODULE: u8 = 19;
    pub const AS: u8 = 20;
    pub const IS: u8 = 21;
    pub const COPY: u8 = 22;
    pub const SIZE_OF: u8 = 23;
    pub const ALIGNMENT_OF: u8 = 24;
    pub const BOX: u8 = 25;
    pub const UNBOX: u8 = 26;
    pub const DYNMETHOD: u8 = 27;
    pub const VAR: u8 = 28;
    pub const VARLOAD: u8 = 29;
    pub const VARSTORE: u8 = 30;
    pub const ARRAYALLOC: u8 = 31;
    pub const ARRAYALLOC_ZONE: u8 = 32;
    pub const ARRAYLOAD: u8 = 33;
    pub const ARRAYSTORE: u8 = 34;
    pub const ARRAYLENGTH: u8 = 35;
    pub const FENCE: u8 = 36;
}

pub mod ty {
    pub const VARARG: u8 = 1;
    pub const BOOL: u8 = 2;
    pub const PTR: u8 = 3;
    pub const CHAR: u8 = 4;
    pub const BYTE: u8 = 5;
    pub const SHORT: u8 = 6;
    pub const INT: u8 = 7;
    pub const LONG: u8 = 8;
    pub const FLOAT: u8 = 9;
    pub const DOUBLE: u8 = 10;
    pub const ARRAY_VALUE: u8 = 11;
    pub const STRUCT_VALUE: u8 = 12;
    pub const FUNCTION: u8 = 13;
    pub const NULL: u8 = 14;
    pub const NOTHING: u8 = 15;
    pub const VIRTUAL: u8 = 16;
    pub const VAR: u8 = 17;
    pub const UNIT: u8 = 18;
    pub const ARRAY: u8 = 19;
    pub const REF: u8 = 20;
    pub const SIZE: u8 = 21;
    pub const INT128: u8 = 22;
}

pub mod val {
    pub const TRUE: u8 = 1;
    pub const FALSE: u8 = 2;
    pub const NULL: u8 = 3;
    pub const ZERO: u8 = 4;
    pub const CHAR: u8 = 5;
    pub const BYTE: u8 = 6;
    pub const SHORT: u8 = 7;
    pub const INT: u8 = 8;
    pub const LONG: u8 = 9;
    pub const FLOAT: u8 = 10;
    pub const DOUBLE: u8 = 11;
    pub const STRUCT_VALUE: u8 = 12;
    pub const ARRAY_VALUE: u8 = 13;
    pub const BYTE_STRING: u8 = 14;
    pub const LOCAL: u8 = 15;
    pub const GLOBAL: u8 = 16;
    pub const UNIT: u8 = 17;
    pub const CONST: u8 = 18;
    pub const STRING: u8 = 19;
    pub const VIRTUAL: u8 = 20;
    pub const CLASS_OF: u8 = 21;
    pub const SIZE: u8 = 23;
    pub const INT128: u8 = 24;
}

pub mod cond {
    pub const SIMPLE: u8 = 1;
    pub const COMPLEX: u8 = 2;
}
