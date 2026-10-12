//! Explicit arguments of implicit parameter clauses (dotty's `Migrations.contextBoundParams` and
//! `implicitParams`), reported by the source version (`report.errorOrMigrationWarning`).

use super::Typer;
use syntax::ast::{Attrs, NodeId};
use syntax::config::SourceVersion;
use syntax::diag::rewrite_notice;
use tasty::TypeId;

/// What a migration reports under the unit's source version.
#[derive(PartialEq)]
pub enum Migration {
    Silent,
    Warning,
    Error,
}

impl Typer<'_> {
    /// The report of a migration warned about from `warn_from` and an error from `error_from`.
    pub fn migration(&self, warn_from: SourceVersion, error_from: SourceVersion) -> Migration {
        let v = self.source_version;
        if v.is_at_least(error_from) && v != error_from.migrating_variant() {
            Migration::Error
        } else if v.is_at_least(warn_from) {
            Migration::Warning
        } else {
            Migration::Silent
        }
    }

    /// Checks arguments `args` of application `id` to a clause of `params`, implicit if
    /// `implicit`, `using` if `contextual`: context bound parameters outside the subset from 3.4,
    /// other implicit ones passed without `using` warned about from 3.7, outside it from `future`.
    pub fn check_clause_args(
        &mut self,
        (id, args): (NodeId, &[NodeId]),
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
        let evidence = params.first().is_some_and(|(n, _)| n.starts_with("evidence$"));
        if evidence && self.migration(SourceVersion::V3_4, SourceVersion::V3_5) != Migration::Silent {
            return Err("explicit arguments to context bound parameters");
        }
        let Some(span) = args.first().map(|&a| self.ast.span(a)).filter(|s| s.exists()) else {
            return Ok(());
        };
        match self.migration(SourceVersion::V3_7, SourceVersion::FUTURE_VERSION) {
            Migration::Silent => return Ok(()),
            Migration::Error => return Err("implicit parameters passed without using"),
            Migration::Warning => {}
        }
        let parens = self.ast.attrs(id).contains(Attrs::PAREN_ARGS);
        let rewrite = if parens { rewrite_notice("This code", "3.7-migration", "") } else { String::new() };
        let msg = format!(
            "Implicit parameters should be provided with a `using` clause.{rewrite}\nTo disable the warning, \
             please use the following option:\n  \"-Wconf:msg=Implicit parameters should be provided with a \
             `using` clause:s\"\n"
        );
        self.diags.warning(span.point(), msg);
        Ok(())
    }
}
