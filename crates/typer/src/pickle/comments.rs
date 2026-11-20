// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/CommentPickler.scala
//   compiler/src/dotty/tools/dotc/util/Spans.scala
//! The Comments section: the doc comment of each definition pickled.

use super::{QuoteSource, Refusal, TreePickler};
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::SymbolId;

/// The bits of a span's start and of its end in its coordinates (dotty's `StartEndBits`).
const START_END_BITS: u32 = 26;

impl TreePickler<'_> {
    /// Notes the doc comment of definition `id` of `sym` if it has one, for `comments`.
    pub fn note_doc_comment(&mut self, id: TNodeId, sym: SymbolId) {
        let comment = self.file.and_then(|f| f.facts.doc_comments.get(&sym).copied());
        if let Some(comment) = comment {
            let key = self.tree_key(id);
            self.doc_comments.push((key, comment));
        }
    }

    /// Writes the Comments section: per documented definition, by address, its address, the
    /// comment's text and its span (dotty's `pickleComments`).
    pub fn comments(&mut self, source: &QuoteSource) -> Result<(), Refusal> {
        let section = self.pickler.new_section("Comments");
        let mut documented: Vec<(usize, Span)> = (self.doc_comments.iter())
            .filter_map(|&(key, comment)| Some((self.buf.addr_of_tree(key)?.0, comment)))
            .collect();
        documented.sort_by_key(|&(at, _)| at);
        let mut buf = tasty::write::Buffer::default();
        for (at, comment) in documented {
            let text = source.text(comment).ok_or("a doc comment outside its file")?;
            let span = source.tasty_span(comment, false)?;
            buf.write_nat(at as u64);
            buf.write_utf8(text);
            buf.write_long_int(i64::from(span.start) | i64::from(span.end) << START_END_BITS);
        }
        self.pickler.set_section(section, buf.bytes);
        Ok(())
    }
}
