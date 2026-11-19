//! Constructs outside the subset and type errors, each with an error tree in its place.

use super::Typer;
use super::implicits::{CANDIDATE_OUTSIDE, NESTED_OUTSIDE};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName};
use syntax::config::{Migration, Severity};
use syntax::diag::{Message, rewrite_notice};
use tasty::Type;

impl Typer<'_> {
    /// Records that `id` is outside the subset; an error tree in its place.
    pub fn unsupported(&mut self, id: NodeId, what: &str) -> TNodeId {
        let span = self.ast.span(id);
        self.unsupported_at(span, what);
        self.error_tree(span)
    }

    /// Records that `id` is outside the subset if a lookup since `before`
    /// (`Universe::refused_lookups`) found a refused class (a jar's Scala 2 class file, which
    /// dotty would read, or a Java source's class the reader refuses): an error tree in its place.
    pub fn lookup_refusal(&mut self, id: NodeId, before: usize) -> Option<TNodeId> {
        if self.u.refused_lookups() == before {
            return None;
        }
        let refusal = self.u.last_refused_lookup()?.to_owned();
        Some(self.unsupported(id, &refusal))
    }

    pub fn unsupported_tree(&mut self, span: Span, what: &str) -> TNodeId {
        self.unsupported_at(span, what);
        self.error_tree(span)
    }

    /// Records that the construct at `span` is outside the subset, an implicit attempt's refusal
    /// with the construct that attempt met.
    pub fn unsupported_at(&mut self, span: Span, what: &str) {
        self.probe_unsupported();
        self.reports += 1;
        if let Some((_, first)) = &mut self.checked_code {
            let at = if span.exists() { 0 } else { self.diags.items.len() };
            *first = Some(first.map_or(at, |f| f.min(at)));
        }
        if self.unsupported.is_none() {
            let what = match self.undone_unsupported.take() {
                Some(inner) if what == CANDIDATE_OUTSIDE || what == NESTED_OUTSIDE => {
                    format!("{what}: {inner}")
                }
                _ => what.to_owned(),
            };
            self.unsupported = Some((span, what));
        }
    }

    pub fn error(&mut self, id: NodeId, msg: impl Into<Message>) -> TNodeId {
        let span = self.ast.span(id);
        self.diags.error_at(span.start(), span.end(), span.point(), msg);
        self.error_tree(span)
    }

    /// Reports an error at `span`, with its point as the position; without a position if `span`
    /// does not exist or is in another source.
    pub fn error_at(&mut self, span: Span, msg: impl Into<Message>) {
        match span.exists() && !span.is_in_other_source() {
            true => self.diags.error_at(span.start(), span.end(), span.point(), msg),
            false => self.diags.error_unpositioned(msg),
        }
    }

    pub fn warning_at(&mut self, span: Span, msg: impl Into<Message>) {
        match span.exists() && !span.is_in_other_source() {
            true => self.diags.warning_at(span.start(), span.end(), span.point(), msg),
            false => self.diags.warning_unpositioned(msg),
        }
    }

    /// Reports `msg` at `span` as an error, a warning or not at all, as the source version
    /// reports legacy construct `m` (dotty's `report.errorOrMigrationWarning`).
    pub fn error_or_migration_warning(&mut self, span: Span, msg: impl Into<Message>, m: Migration) {
        match m.severity(self.source_version) {
            Some(Severity::Error) => self.error_at(span, msg),
            Some(Severity::Warning) => self.warning_at(span, msg),
            None => {}
        }
    }

    /// `diag::rewrite_notice` of `what` under `-source version`, empty in the REPL.
    pub fn rewrite_notice(&self, what: &str, version: &str) -> String {
        if self.interactive { String::new() } else { rewrite_notice(what, version, "") }
    }

    pub fn error_tree(&mut self, span: Span) -> TNodeId {
        let name = TName { name: self.name(names::nme::ERROR), is_type: false };
        let tpe = self.u.intern_type(Type::Error);
        self.tpd.node(T::Ident { name }, span, Some(tpe), None)
    }

    /// Whether the type of tree `t`, widened, is the error type.
    pub fn is_error(&mut self, t: TNodeId) -> bool {
        let tpe = self.u.widen(self.tpd.tpe(t));
        *self.u.ty(tpe) == Type::Error
    }
}
