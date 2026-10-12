//! Explicit arguments of implicit parameter clauses (dotty's `Migrations.contextBoundParams` and
//! `implicitParams`, as of source version 3.7).

use super::Typer;
use syntax::ast::{Attrs, NodeId};
use tasty::TypeId;

impl Typer<'_> {
    /// Checks arguments `args` of application `id` of `fun` to a clause of `params`, implicit
    /// if `implicit`, `using` if `contextual`: an error for context bound parameters, outside
    /// the subset, a warning for other implicit ones passed without `using`.
    pub fn check_clause_args(
        &mut self,
        (id, fun, args): (NodeId, NodeId, &[NodeId]),
        params: &[(String, TypeId)],
        implicit: bool,
        contextual: bool,
    ) -> Result<(), &'static str> {
        let using = self.ast.attrs(id).contains(Attrs::USING_APPLY);
        if contextual && !using || using && !implicit {
            return Err("arguments not matching the clause's using");
        }
        if !implicit || using {
            return Ok(());
        }
        if params.first().is_some_and(|(n, _)| n.starts_with("evidence$")) {
            return Err("explicit arguments to context bound parameters");
        }
        let Some(span) = args.first().map(|&a| self.ast.span(a)).filter(|s| s.exists()) else {
            return Ok(());
        };
        let rewrite = if self.ast.span(fun).end() < span.start() {
            "\nThis code can be rewritten automatically under -rewrite -source 3.7-migration."
        } else {
            ""
        };
        let msg = format!(
            "Implicit parameters should be provided with a `using` clause.{rewrite}\nTo disable the warning, \
             please use the following option:\n  \"-Wconf:msg=Implicit parameters should be provided with a \
             `using` clause:s\"\n"
        );
        self.diags.warning(span.point(), msg);
        Ok(())
    }
}
