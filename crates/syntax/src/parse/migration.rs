//! Legacy syntax, accepted with a warning or rejected by source version (dotty's
//! `MigrationVersion` and `report.errorOrMigrationWarning`).

use super::Parser;
use crate::ast::Span;
use crate::config::SourceVersion as V;

/// A legacy construct and the source versions that warn about it and reject it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Migration {
    Scala2to3,
    ForComprehensionPatternWithoutCase,
    AscriptionAfterPattern,
    ExplicitContextBoundArgument,
    AlphanumericInfix,
    RemoveThisQualifier,
    UninitializedVars,
    VarargSpliceAscription,
    WildcardType,
    WithOperator,
    FunctionUnderscore,
    AmbiguousNamedTupleSyntax,
    ImportWildcard,
    ImportRename,
    ParameterEnclosedByParenthesis,
    XmlLiteral,
    GivenSyntax,
    ImplicitParamsWithoutUsing,
    Scala2Implicits,
    IdentifierDollars,
}

impl Migration {
    /// (warn from, error from); `None` for never.
    fn versions(self) -> (V, Option<V>) {
        use Migration::*;
        let v = |s| V::parse(s).expect("a source version");
        match self {
            Scala2to3 => (v("3.0"), Some(v("3.0"))),
            ForComprehensionPatternWithoutCase => (v("3.2"), Some(v("3.4"))),
            AscriptionAfterPattern => (v("3.3"), Some(v("future"))),
            ExplicitContextBoundArgument => (v("3.4"), Some(v("3.5"))),
            AlphanumericInfix
            | RemoveThisQualifier
            | UninitializedVars
            | VarargSpliceAscription
            | WildcardType
            | WithOperator
            | FunctionUnderscore => (v("3.4"), Some(v("future"))),
            AmbiguousNamedTupleSyntax => (v("3.6"), Some(v("future"))),
            ImportWildcard
            | ImportRename
            | ParameterEnclosedByParenthesis
            | XmlLiteral
            | GivenSyntax
            | Scala2Implicits => (v("future"), Some(v("future"))),
            ImplicitParamsWithoutUsing => (v("3.7"), Some(v("future"))),
            IdentifierDollars => (v("3.9"), None),
        }
    }
}

impl Parser<'_> {
    /// Report `msg` as an error, a migration warning or a warning, depending on the source
    /// version and when `m` became legacy syntax.
    pub fn error_or_migration_warning(&mut self, msg: impl Into<String>, span: Span, m: Migration) {
        let version = self.s.cfg.source_version;
        let (warn_from, error_from) = m.versions();
        if let Some(error_from) = error_from.filter(|&e| version.is_at_least(e)) {
            if version != error_from.migrating_variant() {
                self.syntax_error_span(msg, span);
            } else {
                self.s.diags.warning(span.point(), msg);
            }
        } else if version.is_at_least(warn_from) {
            self.s.diags.warning(span.point(), msg);
        }
    }

    /// `error_or_migration_warning` at a single offset.
    pub fn migration_error(&mut self, msg: impl Into<String>, offset: u32, m: Migration) {
        self.error_or_migration_warning(msg, Span::at(offset), m);
    }
}
