//! The `quotes.reflect` operations of the overlay's `ReflectImpl` by name: terms are expression
//! numbers, types type numbers, symbols the frontend's symbol numbers (`-1` for none), constants
//! a kind, two words and a text.

mod constants;
mod definitions;
mod implicits;
mod members;
mod positions;
mod reports;
mod symbols;
mod terms;
mod trees;
mod type_match;
mod type_reprs;
mod types;

pub use implicits::{FromTyper, SEARCH};
pub use symbols::Made;

use crate::host::Host;
use tasty::SymbolId;
use typer::inlining::Reflect;
use vm::Reflected;

type Answer = Result<Reflected, &'static str>;

/// The refusal of an operation no subject knows.
const UNKNOWN: &str = "a quotes.reflect operation the frontend does not know";

type Run = fn(&mut Host, &str, &[i32], &str) -> Answer;

impl Host {
    /// Runs `quotes.reflect` operation `op` on `args` and `text`.
    pub fn run_reflect(&mut self, op: &str, args: &[i32], text: &str) -> Answer {
        let subjects: [Run; 12] = [
            Host::run_report,
            Host::run_definition,
            Host::run_implicit,
            Host::run_type_match,
            Host::run_type_repr,
            Host::run_member,
            Host::run_tree,
            Host::run_term,
            Host::run_type,
            Host::run_constant,
            Host::run_symbol,
            Host::run_position,
        ];
        for run in subjects {
            match run(self, op, args, text) {
                Err(e) if e == UNKNOWN => continue,
                answer => return answer,
            }
        }
        Err(UNKNOWN)
    }

    fn reflect(&mut self) -> Reflect<'_> {
        let span = self.expansion;
        Reflect { u: &mut self.u, tpd: &mut self.tpd, names: &mut self.names, span }
    }
}

/// Argument `i` of `args`.
fn arg(args: &[i32], i: usize) -> Result<i32, &'static str> {
    args.get(i).copied().ok_or("a quotes.reflect operation without its arguments")
}

/// The answer of numbers `numbers`.
fn numbers(numbers: Vec<i32>) -> Answer {
    Ok((numbers, None))
}

/// The answer of text `s`.
fn text(s: String) -> Answer {
    Ok((Vec::new(), Some(s)))
}

pub fn symbol(n: i32) -> Result<SymbolId, &'static str> {
    (n >= 0).then_some(SymbolId(n as u32)).ok_or("a quotes.reflect operation on Symbol.noSymbol")
}
