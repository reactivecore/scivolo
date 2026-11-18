// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Migrations.scala
//! Explicit arguments of implicit parameter clauses (dotty's `Migrations.contextBoundParams` and
//! `implicitParams`).

use super::Typer;
use syntax::ast::{Attrs, NodeId, Tree};
use syntax::config::SourceVersion;
use tasty::{Kind, NameId, TypeId};

impl Typer<'_> {
    /// Reports type definition `id` if it is named `?`, which was the placeholder of the kind
    /// projector plugin (dotty's `kindProjectorQMark`).
    pub fn kind_projector_qmark(&mut self, id: NodeId) {
        let Tree::TypeDef { name, .. } = *self.ast.tree(id) else { return };
        if self.text(name.name) != "?" {
            return;
        }
        let owner = self.syms.by_tree.get(&id).and_then(|&s| self.u.symbol(s).owner);
        let in_param = owner.is_some_and(|o| self.u.symbol(o).kind == Kind::TypeParam);
        let addendum = if in_param { ", use `_` to denote a higher-kinded type parameter" } else { "" };
        self.error_at(self.ast.span(id), format!("`?` is not a valid type name{addendum}"));
    }

    /// Checks arguments `args` of application `id` to a clause of `params`, implicit if
    /// `implicit`, `using` if `contextual`: context bound parameters outside the subset, other
    /// implicit ones passed without `using` warned about.
    pub fn check_clause_args(
        &mut self,
        (id, args): (NodeId, &[NodeId]),
        params: &[(NameId, TypeId)],
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
        if params.first().is_some_and(|&(n, _)| self.u.name(n).starts_with("evidence$")) {
            return Err("explicit arguments to context bound parameters");
        }
        let Some(span) = args.first().map(|&a| self.ast.span(a)).filter(|s| s.exists()) else {
            return Ok(());
        };
        if !self.source_version.is_at_least(SourceVersion::V3_7) {
            return Ok(());
        }
        let parens = self.ast.attrs(id).contains(Attrs::PAREN_ARGS);
        let rewrite = if parens { self.rewrite_notice("This code", "3.7-migration") } else { String::new() };
        let msg = format!(
            "Implicit parameters should be provided with a `using` clause.{rewrite}\nTo disable the warning, \
             please use the following option:\n  \"-Wconf:msg=Implicit parameters should be provided with a \
             `using` clause:s\"\n"
        );
        self.diags.warning_at(span.start(), span.end(), span.point(), msg);
        Ok(())
    }
}
