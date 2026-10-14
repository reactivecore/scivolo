//! Decodes TASTy files (header, name table, sections, the ASTs section into a node arena) and
//! models a library's symbols and types from them.
#![allow(non_upper_case_globals)]

mod cursor;
mod erasure;
mod file;
mod library;
mod names;
mod positions;
mod print;
mod subtype;
mod symbols;
pub mod tags;
mod tree;
mod types;
mod view;

pub use erasure::{Erased, PRIMITIVE_VALUE_CLASSES};
pub use positions::TastySpan;
pub use subtype::{Mark, NoVars, TypeVars};
pub use symbols::{
    ANY, BodyInfo, Builtins, Error as LoadError, FileId, Flags, Kind, NOTHING, PACKAGE_TABLE, Symbol,
    SymbolId, Universe, encode, is_package_object_name, module_class_name, module_name, package_table,
    package_table_version,
};
pub use types::{Binder, Type, TypeId, Types, VarId, param_tree_clauses};

pub use cursor::{Cursor, Error, Result};
pub use file::{Attribute, Header, Section, Tasty, attribute_name};
pub use names::{Name, NameRef, Names, ParamSig};
pub use print::dump;
pub use tree::{Ast, Node, NodeId, ref_addr};
pub use view::{Const, Parts, View, constant, view};
