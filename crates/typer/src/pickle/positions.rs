// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/PositionPickler.scala
//   compiler/src/dotty/tools/dotc/ast/Positioned.scala
//! The positions of a quote (dotty's `picklePositions`): the span of each tree pickled that its
//! children's spans do not give.

use super::{Refusal, TreePickler};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::source::{Offset, SourceFile};
use tasty::TastySpan;
use tasty::write::{PositionsBuffer, text_line_sizes};

/// The source of a quote: its path from the source root and the size of each of its lines.
pub struct QuoteSource<'a> {
    pub path: String,
    pub line_sizes: Vec<u32>,
    /// The file the quote's spans are in, at its offset among the sources; without it, the spans
    /// are read from the quote's TASTy and count in its UTF-16 units already.
    pub file: Option<(&'a SourceFile, Offset)>,
}

impl<'a> QuoteSource<'a> {
    /// The source `file` at offset `base` among the sources, its path `path`.
    pub fn of(path: String, file: &'a SourceFile, base: Offset) -> QuoteSource<'a> {
        QuoteSource { path, line_sizes: text_line_sizes(&file.text), file: Some((file, base)) }
    }

    /// Offset `o` of a span in UTF-16 units of the source.
    fn units(&self, o: Offset, own: bool) -> Result<u32, Refusal> {
        if !own {
            return Ok(o);
        }
        let (file, base) = self.file.ok_or("a span of the quote's source without the source")?;
        let text = o.checked_sub(base).and_then(|o| file.text.get(..o as usize));
        let text = text.ok_or("a quote with a span in another of the sources")?;
        Ok(if text.is_ascii() { text.len() as u32 } else { text.encode_utf16().count() as u32 })
    }

    fn tasty_span(&self, span: Span) -> Result<TastySpan, Refusal> {
        let span = span.without_inlined_call();
        let own = !span.is_in_other_source();
        if !own && self.file.is_some() {
            return Err("a quote with a span in a library's source");
        }
        let span = span.in_own_source();
        let point = (!span.is_synthetic()).then(|| self.units(span.point(), own)).transpose()?;
        Ok(TastySpan { start: self.units(span.start(), own)?, end: self.units(span.end(), own)?, point })
    }
}

impl TreePickler<'_> {
    /// The Positions section of quote `root` of `source`, if `root` has a span.
    pub fn positions(&mut self, root: TNodeId, source: &QuoteSource) -> Result<(), Refusal> {
        if !self.tpd.span(root).exists() {
            return Ok(());
        }
        let section = self.pickler.new_section("Positions");
        let mut out = PositionsBuffer::new(&source.line_sizes);
        let mut todo = vec![(root, true)];
        while let Some((t, first)) = todo.pop() {
            let span = self.tpd.span(t);
            if let Some(addr) = self.buf.addr_of_tree(t.index() as u32).filter(|_| span.exists()) {
                if first {
                    out.span(addr, source.tasty_span(span)?);
                    out.source(self.pickler.names.utf8(&source.path));
                } else if !out.is_pickled(addr)
                    && (span.to_synthetic() != self.envelope(t) || self.needs_pos(t))
                {
                    out.span(addr, source.tasty_span(span)?);
                }
            }
            let hole_args = self.holes.get(&t).map(|(_, args)| args.clone()).unwrap_or_default();
            let annots = match self.tpd.sym(t).filter(|_| self.needs_pos(t)) {
                Some(s) => self.tpd.annotations(s).to_vec(),
                None => Vec::new(),
            };
            let own = self.tpd.children(t);
            let children = annots.into_iter().chain(own.iter().copied()).chain(hole_args).collect::<Vec<_>>();
            todo.extend(children.into_iter().rev().map(|c| (c, false)));
        }
        self.pickler.set_section(section, out.buf.bytes);
        Ok(())
    }

    /// The span enclosing `t`'s children, synthetic, else an inlined tree's call's (dotty's
    /// `Positioned.envelope`).
    fn envelope(&self, t: TNodeId) -> Span {
        if let T::Inlined { call, .. } = *self.tpd.tree(t) {
            return call.map_or(Span::NONE, |c| self.tpd.span(c));
        }
        let spans = self.tpd.children(t).iter().map(|&c| self.tpd.span(c)).collect::<Vec<_>>();
        spans.into_iter().fold(Span::NONE, Span::union).to_synthetic()
    }

    /// Whether `t`'s span is written even where its children give it (dotty's `alwaysNeedsPos`).
    fn needs_pos(&self, t: TNodeId) -> bool {
        let defines = matches!(
            self.tpd.tree(t),
            T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Bind { .. } | T::Template { .. }
        );
        defines || matches!(self.tpd.tree(t), T::PackageDef { .. }) || self.holes.contains_key(&t)
    }
}
