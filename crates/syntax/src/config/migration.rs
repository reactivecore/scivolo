// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/config/MigrationVersion.scala
//   compiler/src/dotty/tools/dotc/report.scala
//! Legacy constructs and the source versions that warn about them and reject them (dotty's
//! `MigrationVersion`).

use super::SourceVersion as V;

/// A legacy construct and the source versions that warn about it and reject it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Migration {
    Scala2to3,
    ForComprehensionPatternWithoutCase,
    ForComprehensionUncheckedPathDefs,
    AscriptionAfterPattern,
    ExplicitContextBoundArgument,
    AlphanumericInfix,
    RemoveThisQualifier,
    UninitializedVars,
    VarargSpliceAscription,
    WildcardType,
    WithOperator,
    FunctionUnderscore,
    NonNamedArgumentInJavaAnnotation,
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

/// How a legacy construct is reported.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    Warning,
}

impl Migration {
    /// (warn from, error from); `None` for never.
    fn versions(self) -> (V, Option<V>) {
        use Migration::*;
        let v = |s| V::parse(s).expect("a source version");
        match self {
            Scala2to3 => (v("3.0"), Some(v("3.0"))),
            ForComprehensionPatternWithoutCase => (v("3.2"), Some(v("3.4"))),
            ForComprehensionUncheckedPathDefs => (v("3.2"), Some(v("future"))),
            AscriptionAfterPattern => (v("3.3"), Some(v("future"))),
            ExplicitContextBoundArgument => (v("3.4"), Some(v("3.5"))),
            AlphanumericInfix
            | RemoveThisQualifier
            | UninitializedVars
            | VarargSpliceAscription
            | WildcardType
            | WithOperator
            | FunctionUnderscore => (v("3.4"), Some(v("future"))),
            NonNamedArgumentInJavaAnnotation => (v("3.6"), Some(v("3.6"))),
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

    /// Determines how source version `version` reports the construct (dotty's
    /// `report.errorOrMigrationWarning`): an error from the version that rejects it, a warning in
    /// that version's `-migration` variant and from the version that warns about it; nothing
    /// before that or under `2.13`.
    pub fn severity(self, version: V) -> Option<Severity> {
        if version.is_scala2() {
            return None;
        }
        let (warn_from, error_from) = self.versions();
        match error_from.filter(|&e| version.is_at_least(e)) {
            Some(e) if version != e.migrating_variant() => Some(Severity::Error),
            Some(_) => Some(Severity::Warning),
            None => version.is_at_least(warn_from).then_some(Severity::Warning),
        }
    }
}
