// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Flags.scala
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/PickleBuffer.scala
//! The flags of a symbol in a Scala 2 pickle, at the bits the pickle has them (the `*_PKL`
//! positions for the low twelve).
#![allow(dead_code)]

pub const IMPLICIT: u64 = 1 << 0;
pub const FINAL: u64 = 1 << 1;
pub const PRIVATE: u64 = 1 << 2;
pub const PROTECTED: u64 = 1 << 3;
pub const SEALED: u64 = 1 << 4;
pub const OVERRIDE: u64 = 1 << 5;
pub const CASE: u64 = 1 << 6;
pub const ABSTRACT: u64 = 1 << 7;
pub const DEFERRED: u64 = 1 << 8;
pub const METHOD: u64 = 1 << 9;
pub const MODULE: u64 = 1 << 10;
pub const INTERFACE: u64 = 1 << 11;
pub const MUTABLE: u64 = 1 << 12;
pub const PARAM: u64 = 1 << 13;
pub const PACKAGE: u64 = 1 << 14;
pub const MACRO: u64 = 1 << 15;
/// `BYNAMEPARAM` on a term.
pub const COVARIANT: u64 = 1 << 16;
pub const CONTRAVARIANT: u64 = 1 << 17;
pub const ABSOVERRIDE: u64 = 1 << 18;
pub const LOCAL: u64 = 1 << 19;
pub const JAVA: u64 = 1 << 20;
pub const SYNTHETIC: u64 = 1 << 21;
pub const STABLE: u64 = 1 << 22;
pub const STATIC: u64 = 1 << 23;
pub const CASEACCESSOR: u64 = 1 << 24;
/// `DEFAULTPARAM` on a term.
pub const TRAIT: u64 = 1 << 25;
pub const BRIDGE: u64 = 1 << 26;
pub const ACCESSOR: u64 = 1 << 27;
pub const SUPERACCESSOR: u64 = 1 << 28;
pub const PARAMACCESSOR: u64 = 1 << 29;
pub const MODULEVAR: u64 = 1 << 30;
pub const LAZY: u64 = 1 << 31;
/// `EXISTENTIAL` on a type.
pub const MIXEDIN: u64 = 1 << 35;
pub const EXPANDEDNAME: u64 = 1 << 36;
pub const SPECIALIZED: u64 = 1 << 40;
pub const VARARGS: u64 = 1 << 43;
pub const ARTIFACT: u64 = 1 << 46;
pub const ENUM: u64 = 1 << 48;
