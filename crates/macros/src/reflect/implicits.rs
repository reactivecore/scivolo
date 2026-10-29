// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The implicit searches of `quotes.reflect`, which the typer answers between runs of the
//! expansion (`Implicits.search`), and `CompilationInfo`.

use super::Fallible;
use crate::host::Host;
use tasty::TypeId;
use typer::inlining::Annotations;
use typer::tpd::TNodeId;

/// What the typer gives an expansion: the unit's annotations, the implicit searches answered
/// so far and whether it expands while typing; the search the expansion asks for next.
#[derive(Default)]
pub struct FromTyper {
    pub annotations: Annotations,
    pub searched: Vec<(TypeId, Option<TNodeId>)>,
    pub while_typing: bool,
    pub wanted: Option<TypeId>,
}

/// The refusal that stops a run asking for a search.
pub const SEARCH: &str = "an implicit search the typer answers";

impl Host {
    pub fn is_while_typing(&self) -> bool {
        self.typer.while_typing
    }

    pub fn macro_settings(&self) -> String {
        self.settings.macro_settings.join("\n")
    }

    /// The implicit argument of type `pt` found as `[1, tree]`, `[0]` for none.
    pub fn search(&mut self, pt: TypeId) -> Fallible<Vec<i32>> {
        let answered = self.typer.searched.iter().find(|&&(t, _)| t == pt).map(|&(_, found)| found);
        match answered {
            Some(Some(t)) => Ok(vec![1, self.add_tree(t) as i32]),
            Some(None) => Ok(vec![0]),
            None => {
                self.typer.wanted.get_or_insert(pt);
                Err(SEARCH)
            }
        }
    }
}
