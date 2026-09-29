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
    /// dotty's `Message.rewriteNotice`, appended to messages about rewritable syntax.
    pub fn rewrite_notice(what: &str, version: &str, options: &str) -> String {
        let source = if version.is_empty() { String::new() } else { format!("-source {version}") };
        let opts = match (source.is_empty(), options.is_empty()) {
            (_, true) => source,
            (true, false) => options.to_string(),
            (false, false) => format!("{source} {options}"),
        };
        format!("\n{what} can be rewritten automatically under -rewrite {opts}.")
    }

    /// An error for syntax that `option` disallows.
    pub fn syntax_version_error(&mut self, option: &str, span: Span) {
        let notice = Self::rewrite_notice("This construct", "3.0-migration", option);
        self.syntax_error_span(format!("This construct is not allowed under {option}.{notice}"), span);
    }

    /// Under `-new-syntax` (or `-source future`), old syntax at `span` is an error.
    pub fn rewrite_to_new_syntax(&mut self, span: Span) {
        if self.new_syntax {
            self.syntax_version_error("-new-syntax or -language:future", span);
        }
    }

    /// Under `-old-syntax`, new syntax at `span` is an error.
    pub fn rewrite_to_old_syntax(&mut self, span: Span) {
        if self.s.cfg.old_syntax {
            self.syntax_version_error("-old-syntax", span);
        }
    }

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
