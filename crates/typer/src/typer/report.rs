//! Constructs outside the subset and type errors, each with an error tree in its place.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName};
use syntax::diag::Message;
use tasty::Type;

impl Typer<'_> {
    /// Records that `id` is outside the subset; an error tree in its place.
    pub fn unsupported(&mut self, id: NodeId, what: &str) -> TNodeId {
        let span = self.ast.span(id);
        self.unsupported_at(span, what);
        self.error_tree(span)
    }

    pub fn unsupported_at(&mut self, span: Span, what: &str) {
        self.probe_unsupported();
        if self.unsupported.is_none() {
            self.unsupported = Some((span, what.to_owned()));
        }
    }

    pub fn error(&mut self, id: NodeId, msg: impl Into<Message>) -> TNodeId {
        let span = self.ast.span(id);
        self.diags.error_at(span.start(), span.end(), span.point(), msg);
        self.error_tree(span)
    }

    /// Reports an error at `span`, with its point as the position.
    pub fn error_at(&mut self, span: Span, msg: impl Into<Message>) {
        self.diags.error_at(span.start(), span.end(), span.point(), msg);
    }

    pub fn error_tree(&mut self, span: Span) -> TNodeId {
        let name = TName { name: self.name("<error>"), is_type: false };
        let tpe = self.u.intern_type(Type::Error);
        self.tpd.node(T::Ident { name }, span, Some(tpe), None)
    }

    /// Whether the type of tree `t`, widened, is the error type.
    pub fn is_error(&mut self, t: TNodeId) -> bool {
        let tpe = self.u.widen(self.tpd.tpe(t));
        *self.u.types().get(tpe) == Type::Error
    }
}
