//! The doc comment of each definition, by tree (dotty's `DocComment` attachment).

use super::{Ast, NodeId, Span};

impl Ast {
    /// Attaches doc comment `comment`, its span, to definition `md` (dotty's `setComment`).
    pub fn set_doc_comment(&mut self, md: NodeId, comment: Span) {
        self.doc_comments.insert(md, comment);
    }

    /// Gets the span of the doc comment of definition `md` (dotty's `rawComment`).
    pub fn doc_comment(&self, md: NodeId) -> Option<Span> {
        self.doc_comments.get(&md).copied()
    }

    /// Lists the definitions with a doc comment, each with the span of its comment.
    pub fn doc_comments(&self) -> impl Iterator<Item = (NodeId, Span)> + '_ {
        self.doc_comments.iter().map(|(&md, &comment)| (md, comment))
    }
}
