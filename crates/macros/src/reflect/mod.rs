//! The `quotes.reflect` operations of the overlay's `ReflectImpl` by name: terms are expression
//! numbers, types type numbers, symbols the frontend's symbol numbers (`-1` for none), constants
//! a kind, two words and a text.

mod classes;
mod constants;
mod copies;
mod declarations;
mod decode;
mod definitions;
mod encode;
mod implicits;
mod lambda_types;
mod lambda_variances;
mod made;
mod members;
mod ops;
mod overloads;
mod positions;
mod reports;
mod signatures;
mod symbols;
mod terms;
mod tree_kinds;
mod tree_parts;
mod trees;
mod type_kinds;
mod type_match;
mod type_members;
mod type_reprs;
mod types;

pub use implicits::{FromTyper, SEARCH};
pub use symbols::Made;

use crate::host::Host;
use typer::inlining::Reflect;
use vm::Reflected;

type Fallible<T> = Result<T, &'static str>;

type Answer = Fallible<Reflected>;

/// The refusal of an operation not in the table of `ops`.
const UNKNOWN: &str = "a quotes.reflect operation the frontend does not know";

impl Host {
    fn reflect(&mut self) -> Reflect<'_> {
        let span = self.expansion;
        Reflect { u: &mut self.u, tpd: &mut self.tpd, names: &mut self.names, span }
    }
}
