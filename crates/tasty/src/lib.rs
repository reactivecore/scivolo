//! Decodes TASTy files: header, name table, sections, and the ASTs section into a node arena.
#![allow(non_upper_case_globals)]

mod cursor;
mod file;
mod library;
mod names;
mod print;
mod symbols;
pub mod tags;
mod tree;
mod view;

pub use symbols::{Error as LoadError, Flags, Kind, Symbol, SymbolId, Universe};

pub use cursor::{Cursor, Error, Result};
pub use file::{Attribute, Header, Section, Tasty, attribute_name};
pub use names::{Name, NameRef, Names, ParamSig};
pub use print::dump;
pub use tree::{Ast, Node, NodeId, ref_addr};
pub use view::{Const, Parts, View, constant, view};
