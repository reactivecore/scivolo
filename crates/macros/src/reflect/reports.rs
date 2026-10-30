// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The errors and warnings of `quotes.reflect.report`, recorded with their spans.

use super::Fallible;
use crate::host::Host;
use syntax::ast::Span;
use syntax::diag::Level;
use typer::tpd::TNodeId;

impl Host {
    /// Reports `message` at `at`, at the expansion for none.
    pub fn error(&mut self, at: Option<TNodeId>, message: String) {
        let span = self.report_span(at);
        self.reported.push((Level::Error, span, message));
    }

    /// Reports `message` at offsets `start` until `end` of source file `file`.
    pub fn error_at(&mut self, file: i32, start: i32, end: i32, message: String) -> Fallible<()> {
        let span = self.span_of((file, start, end))?;
        self.reported.push((Level::Error, span, message));
        Ok(())
    }

    /// Warns `message` at `at`, at the expansion for none.
    pub fn warning(&mut self, at: Option<TNodeId>, message: String) {
        let span = self.report_span(at);
        self.reported.push((Level::Warning, span, message));
    }

    /// Warns `message` at offsets `start` until `end` of source file `file`.
    pub fn warning_at(&mut self, file: i32, start: i32, end: i32, message: String) -> Fallible<()> {
        let span = self.span_of((file, start, end))?;
        self.reported.push((Level::Warning, span, message));
        Ok(())
    }

    fn report_span(&self, at: Option<TNodeId>) -> Span {
        match at {
            Some(t) => self.tpd.span(t).without_inlined_call(),
            None => self.expansion,
        }
    }
}
