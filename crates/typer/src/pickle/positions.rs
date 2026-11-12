// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/PositionPickler.scala
//   compiler/src/dotty/tools/dotc/ast/Positioned.scala
//! The positions of a quote (dotty's `picklePositions`): the span of each tree pickled that its
//! children's spans do not give.

use super::{Refusal, TreePickler};
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use syntax::source::{Offset, SourceFile};
use tasty::TastySpan;
use tasty::write::{Addr, PositionsBuffer, text_line_sizes};

/// The first id of the trees registered that have no node.
const SYNTHETIC: u32 = u32::MAX / 2;

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

    /// The TASTy span of `span`, in a library's source only if `library` is known.
    fn tasty_span(&self, span: Span, library: bool) -> Result<TastySpan, Refusal> {
        let span = span.without_inlined_call();
        let own = !span.is_in_other_source();
        if !own && self.file.is_some() && !library {
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
        let mut paths = vec![source.path.clone()];
        // Each tree with the index in `paths` of its parent's source, if any, and of the source
        // of the inline expansion it is in, if any.
        let mut todo = vec![(root, None, None)];
        let mut nodes: Vec<usize> =
            self.buf.trees().filter(|&(id, _)| id < SYNTHETIC).map(|(_, a)| a.0).collect();
        nodes.sort_unstable();
        // Each tree without a node, in the order dotty writes them, before the node pickled next
        // or after the node given.
        let mut synthetic: Vec<(usize, Addr, Span)> = self
            .synthetic
            .iter()
            .rev()
            .filter(|&&(_, _, after)| after.is_none())
            .filter_map(|&(id, s, _)| {
                let at = self.buf.addr_of_tree(id)?;
                Some((nodes.get(nodes.partition_point(|&n| n <= at.0)).copied().unwrap_or(usize::MAX), at, s))
            })
            .collect();
        let after: FxHashMap<TNodeId, (Addr, Span)> = self
            .synthetic
            .iter()
            .filter_map(|&(id, s, after)| Some((after?, (self.buf.addr_of_tree(id)?, s))))
            .collect();
        while let Some((t, current, library)) = todo.pop() {
            let span = self.tpd.span(t);
            let other = span.exists() && span.without_inlined_call().is_in_other_source();
            let own = match other && self.file.is_some() {
                true => library,
                false => Some(0),
            };
            let in_library = own.is_some_and(|i| i > 0);
            let key = self.tpd.copy_of(t).unwrap_or(t).index() as u32;
            let at = self.buf.addr_of_tree(key).map(|a| a.0);
            while let Some(&(_, addr, span)) = synthetic.last().filter(|&&(next, _, _)| Some(next) == at) {
                out.span(addr, source.tasty_span(span, false)?);
                synthetic.pop();
            }
            if let Some(addr) = self.buf.addr_of_tree(key).filter(|_| span.exists()) {
                if own != current {
                    out.span(addr, source.tasty_span(span, in_library)?);
                    let path = own.map_or(&source.path, |i| &paths[i]);
                    out.source(self.pickler.names.utf8(path));
                } else if !out.is_pickled(addr)
                    && (span.to_synthetic() != self.envelope(t) || self.needs_pos(t))
                {
                    out.span(addr, source.tasty_span(span, in_library)?);
                }
            }
            if let Some(addr) =
                self.buf.addr_of_tree(t.index() as u32).filter(|_| self.self_refs.contains(&t))
            {
                out.span(Addr(addr.0 + 1), source.tasty_span(span.start_pos().to_synthetic(), in_library)?);
            }
            if let Some(&(addr, span)) = after.get(&t) {
                out.span(addr, source.tasty_span(span, false)?);
            }
            let current = if span.exists() { own } else { current };
            let library = self.inline_source(t).map_or(library, |path| Some(source_index(&mut paths, path)));
            let hole_args = self.holes.get(&t).map(|(_, args)| args.clone()).unwrap_or_default();
            let annots = match self.tpd.sym(t).filter(|_| self.needs_pos(t)) {
                Some(s) => self.annotation_trees(s),
                None => Vec::new(),
            };
            let own: Vec<TNodeId> = match self.call_traces.contains(&t) {
                true => Vec::new(),
                false => self.tpd.children(t).iter().copied().collect(),
            };
            let children = annots.into_iter().chain(own).chain(hole_args).collect::<Vec<_>>();
            for c in children.into_iter().rev() {
                let read = self.tpd.read_from(c).map(|_| source_index(&mut paths, String::new()));
                todo.push((c, current, read.or(library)));
            }
        }
        while let Some((_, addr, span)) = synthetic.pop() {
            out.span(addr, source.tasty_span(span, false)?);
        }
        for (apply, new, tpt) in self.annots.clone() {
            let span = self.tpd.span(tpt);
            let addrs = (self.buf.addr_of_tree(apply), self.buf.addr_of_tree(new));
            if let (Some(apply), Some(new)) = addrs
                && span.exists()
            {
                out.span(apply, source.tasty_span(span, false)?);
                out.source(self.pickler.names.utf8(&source.path));
                out.span(new, source.tasty_span(span.start_pos(), false)?);
            }
        }
        self.pickler.set_section(section, out.buf.bytes);
        Ok(())
    }

    /// Gives the tree pickled next, which has no node, span `span` if it exists; its position
    /// after node `after`'s if given.
    pub fn synthetic_here(&mut self, span: Span, after: Option<TNodeId>) {
        if span.exists() && !self.keying {
            let id = SYNTHETIC + self.synthetic.len() as u32;
            self.buf.register_tree(id);
            self.synthetic.push((id, span, after));
        }
    }

    /// The source path of the library inline method whose expansion `t` is, if it is one.
    fn inline_source(&self, t: TNodeId) -> Option<String> {
        let T::Inlined { call: Some(mut call), .. } = *self.tpd.tree(t) else { return None };
        while let T::Apply { fun, .. } | T::TypeApply { fun, .. } = *self.tpd.tree(call) {
            call = fun;
        }
        self.u.source_path(self.tpd.sym(call)?)
    }

    /// The span enclosing `t`'s children, synthetic, else an inlined tree's call's (dotty's
    /// `Positioned.envelope`).
    fn envelope(&self, t: TNodeId) -> Span {
        if let T::Inlined { call, .. } = *self.tpd.tree(t) {
            return call.map_or(Span::NONE, |c| self.tpd.span(c));
        }
        if self.call_traces.contains(&t) {
            return Span::NONE;
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

/// The index of source `path` in `paths`, added if missing.
fn source_index(paths: &mut Vec<String>, path: String) -> usize {
    paths.iter().position(|p| *p == path).unwrap_or_else(|| {
        paths.push(path);
        paths.len() - 1
    })
}
