//! TASTy reader spike: header, name table, sections and a structural decode of the ASTs section.
#![allow(non_upper_case_globals)]

mod cursor;
mod file;
mod names;
mod print;
pub mod tags;
mod tree;
mod view;

pub use cursor::{Cursor, Error, Result};
pub use file::{Attribute, Header, Section, Tasty, attribute_name};
pub use names::{Name, NameRef, Names, ParamSig};
pub use print::dump;
pub use tree::{Ast, Node, NodeId, ref_addr};
pub use view::{Const, Parts, View, constant, view};
