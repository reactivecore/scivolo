//! Constructs outside the subset and type errors, each with an error tree in its place.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName};
use syntax::diag::{Message, rewrite_notice};
use tasty::Type;

impl Typer<'_> {
    /// Records that `id` is outside the subset; an error tree in its place.
    pub fn unsupported(&mut self, id: NodeId, what: &str) -> TNodeId {
        let span = self.ast.span(id);
        self.unsupported_at(span, what);
        self.error_tree(span)
    }

    pub fn unsupported_tree(&mut self, span: Span, what: &str) -> TNodeId {
        self.unsupported_at(span, what);
        self.error_tree(span)
    }

    pub fn unsupported_at(&mut self, span: Span, what: &str) {
        self.probe_unsupported();
        self.reports += 1;
        if let Some((_, first)) = &mut self.checked_code {
            let at = if span.exists() { 0 } else { self.diags.items.len() };
            *first = Some(first.map_or(at, |f| f.min(at)));
        }
        if self.unsupported.is_none() {
            self.unsupported = Some((span, what.to_owned()));
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
