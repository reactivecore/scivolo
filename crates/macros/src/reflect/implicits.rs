// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The implicit searches of `quotes.reflect`, which the typer answers between runs of the
//! expansion (`Implicits.search`).

use super::{Answer, UNKNOWN, arg, numbers};
use crate::host::Host;
use tasty::TypeId;
use typer::inlining::Annotations;
use typer::tpd::TNodeId;

/// What the typer gives an expansion: the unit's annotations and the implicit searches answered
/// so far; the search the expansion asks for next.
#[derive(Default)]
pub struct FromTyper {
    pub annotations: Annotations,
    pub searched: Vec<(TypeId, Option<TNodeId>)>,
    pub wanted: Option<TypeId>,
}

/// The refusal that stops a run asking for a search.
pub const SEARCH: &str = "an implicit search the typer answers";

impl Host {
    /// Runs implicit operation `op` on `args`: the argument found as `[1, tree]`, `[0]` for none.
    pub fn run_implicit(&mut self, op: &str, args: &[i32], _: &str) -> Answer {
        if op != "search" {
            return Err(UNKNOWN);
        }
        let pt = self.type_of(arg(args, 0)? as u32)?;
        let answered = self.typer.searched.iter().find(|&&(t, _)| t == pt).map(|&(_, found)| found);
        match answered {
            Some(Some(t)) => numbers(vec![1, self.add_tree(t) as i32]),
            Some(None) => numbers(vec![0]),
            None => {
                self.typer.wanted.get_or_insert(pt);
                Err(SEARCH)
            }
        }
    }
}
