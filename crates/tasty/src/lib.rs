//! Decodes TASTy files (header, name table, sections, the ASTs section into a node arena) and
//! models a library's symbols and types from them.
#![allow(non_upper_case_globals)]

mod cursor;
mod erasure;
mod file;
mod library;
mod names;
mod print;
mod symbols;
pub mod tags;
mod tree;
mod types;
mod view;

pub use erasure::Erased;
pub use symbols::{Error as LoadError, FileId, Flags, Kind, Symbol, SymbolId, Universe, encode};
pub use types::{Binder, Type, TypeId, Types};

pub use cursor::{Cursor, Error, Result};
pub use file::{Attribute, Header, Section, Tasty, attribute_name};
pub use names::{Name, NameRef, Names, ParamSig};
pub use print::dump;
pub use tree::{Ast, Node, NodeId, ref_addr};
pub use view::{Const, Parts, View, constant, view};
