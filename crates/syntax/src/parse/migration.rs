// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Parsers.scala
//   compiler/src/dotty/tools/dotc/report.scala
//! Legacy syntax, accepted with a warning or rejected by source version (dotty's
//! `report.errorOrMigrationWarning`).

use super::Parser;
use crate::ast::Span;
pub use crate::config::Migration;
use crate::config::Severity;
use crate::diag::{Message, rewrite_notice};

impl Parser<'_> {
    /// An error for syntax that `option` disallows.
    pub fn syntax_version_error(&mut self, option: &str, span: Span) {
        let notice = rewrite_notice("This construct", "3.0-migration", option);
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
    /// version and when `m` became legacy syntax; nothing under `2.13`.
    pub fn error_or_migration_warning(&mut self, msg: impl Into<Message>, span: Span, m: Migration) {
        match m.severity(self.s.cfg.source_version) {
            Some(Severity::Error) => self.syntax_error_span(msg, span),
            Some(Severity::Warning) => self.s.diags.warning_at(span.start(), span.end(), span.point(), msg),
            None => {}
        }
    }

    /// `error_or_migration_warning` at a single offset.
    pub fn migration_error(&mut self, msg: impl Into<Message>, offset: u32, m: Migration) {
        self.error_or_migration_warning(msg, Span::at(offset), m);
    }
}
