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
pub mod write;

pub use erasure::{Erased, PRIMITIVE_VALUE_CLASSES};
pub use positions::{TastySpan, decode_positions, line_sizes};
pub use subtype::{Changes, Mark, NoVars, NullMode, TypeVars};
pub use symbols::{
    ANY, BodyInfo, Defn, Error as LoadError, FileId, Flags, Kind, MAX_IMPLEMENTED_ARITY, NOTHING,
    PACKAGE_TABLE, Retention, SYMBOL_TABLE_VERSION, Symbol, SymbolId, Universe, decode, defined_name, defn,
    encode, is_package_object_name, module_class_name, module_name, package_table, package_table_version,
    symbol_table,
};
pub use types::{
    Binder, Binding, Highlight, NameId, ParamList, Type, TypeId, TypeList, Types, VarId, escape,
    param_tree_clauses,
};

pub use cursor::{Cursor, Error, Result};
pub use file::{Attribute, Header, Section, Tasty, attribute_name};
pub use names::{Name, NameRef, Names, ParamSig};
pub use print::dump;
pub use tree::{Ast, Node, NodeId, ref_addr};
pub use view::{Const, Parts, View, constant, view};
