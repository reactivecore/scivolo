//! Completion at a tree of a unit (dotty's `Completion.completions`): the unit is typed with a
//! probe at the tree, then the names offered there are computed in the same typer.

mod members;
mod probe;

use crate::Units;
use crate::typer::{Pt, Typer};
use syntax::ast::{Ast, NodeId, Span};
use syntax::names::{FreshNames, Names};
use tasty::Universe;

pub use probe::Probe;

/// A completion of the selection `target` with names starting with `prefix`.
pub struct Request {
    pub target: NodeId,
    pub prefix: String,
}

impl Units {
    /// The names dotty offers for `request` in unit `root`, typed as `type_unit` types it, its
    /// symbols kept as `type_unit` keeps them; none where they are not all known. A panic
    /// leaves these units' script symbols as they were, not `u`, `names` and `fresh`.
    pub fn complete(
        &mut self,
        u: &mut Universe,
        (ast, root): (&mut Ast, NodeId),
        (names, fresh): (&mut Names, &mut FreshNames),
        cfg: syntax::config::Config,
        request: &Request,
    ) -> Vec<String> {
        let mut syms = self.syms.clone();
        let Ok(mut typer) = Typer::new(u, (ast, root), (names, fresh), cfg, &mut syms) else {
            return Vec::new();
        };
        typer.probe = Some(Probe::new(request.target));
        let ctx = crate::root_context(&mut typer);
        typer.typed(root, Pt::Any, ctx);
        if let Err(what) = typer.zonk_trees() {
            typer.unsupported_at(Span::NONE, what);
        }
        let found = typer.completions(&request.prefix).unwrap_or_default();
        self.syms = std::mem::take(&mut typer.syms);
        self.syms.end_unit(typer.u);
        found
    }
}
