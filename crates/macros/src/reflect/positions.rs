//! The positions of `quotes.reflect`: a position is the number of its source file and its two
//! offsets in that file.

use super::{Answer, Fallible};
use crate::host::Host;
use syntax::ast::Span;
use syntax::source::{Offset, SourceFile, Sources};
use tasty::TypeId;
use typer::tpd::{TNodeId, Tree as T};

impl Host {
    pub fn expansion_pos(&self) -> Answer {
        self.located(self.expansion)
    }

    pub fn pos(&self, t: TNodeId) -> Answer {
        self.located(self.tpd.span(t))
    }

    /// `TypeTree.of[T]` of `Type` number `n`: a type tree of its type, at the span of its type
    /// tree if the macro got it as an argument.
    pub fn type_tree_of(&mut self, n: i32) -> Fallible<TNodeId> {
        let t = self.type_of(n as u32)?;
        let span = self.type_arg_spans.get(&(n as u32)).copied().unwrap_or(Span::NONE);
        Ok(self.tpd.node(T::TypeTree { inferred: false }, span, Some(t), None))
    }

    /// `TypeRepr.asType` of type number `n`: its type under a number of its own, without the span
    /// of a type tree.
    pub fn as_type(&mut self, n: i32) -> Fallible<TypeId> {
        self.type_shape_of(n as u32)
    }

    /// The lines and columns of offsets `start` and `end` of source file `file`, and the code
    /// between them.
    pub fn position(&self, file: i32, start: i32, end: i32) -> Answer {
        let f = self.source(file)?;
        if start < 0 || end < start {
            return Err("a position with offsets out of order");
        }
        let byte = |units: i32| f.advance_utf16(0, units as usize);
        let at = |units: i32| {
            let line = f.offset_to_line(byte(units));
            [line as i32, units - utf16(f, f.line_to_offset(line))]
        };
        let code = f.text.get(byte(start) as usize..byte(end) as usize).map(|c| c.encode_utf16().collect());
        Ok(([at(start), at(end)].concat(), code))
    }

    pub fn source_path(&self, file: i32) -> Fallible<String> {
        Ok(self.source(file)?.name.clone())
    }

    pub fn source_content(&self, file: i32) -> Fallible<String> {
        Ok(self.source(file)?.text.clone())
    }

    /// Determines the index of the source file of the unit the expansion is in.
    pub fn expansion_file(&self) -> usize {
        let at = self.expansion.in_own_source().without_inlined_call();
        Sources::new(&self.sources).map_or(0, |sources| sources.index(at.start()))
    }

    fn source(&self, file: i32) -> Fallible<&SourceFile> {
        self.sources.get(file as usize).ok_or("a position in no source file")
    }

    /// `span` as the number of its source file and its offsets in that file, in UTF-16 units.
    pub fn located(&self, span: Span) -> Answer {
        if !span.exists() || span.is_in_other_source() || self.sources.is_empty() {
            return Err("a position outside the sources of the unit");
        }
        let span = span.without_inlined_call();
        let sources = Sources::new(&self.sources).map_err(|_| "sources of more than 1 GiB")?;
        let i = sources.index(span.start());
        let (base, f) = sources.files()[i];
        Ok((vec![i as i32, utf16(f, span.start() - base), utf16(f, span.end() - base)], None))
    }

    /// The span of offsets `start` until `end`, in UTF-16 units, of source file number `file`.
    pub fn span_of(&self, (file, start, end): (i32, i32, i32)) -> Result<Span, &'static str> {
        let sources = Sources::new(&self.sources).map_err(|_| "sources of more than 1 GiB")?;
        let &(base, f) = sources.files().get(file as usize).ok_or("a position in no source file")?;
        if start < 0 || end < start {
            return Err("a position with offsets out of order");
        }
        let byte = |units: i32| base + f.advance_utf16(0, units as usize);
        Ok(Span::new(byte(start), byte(end), byte(start)))
    }
}

/// The UTF-16 units of `f` before byte offset `offset`.
fn utf16(f: &SourceFile, offset: Offset) -> i32 {
    let (text, past) = match f.text.get(..offset as usize) {
        Some(text) => (text, 0),
        None => (f.text.as_str(), offset.saturating_sub(f.len())),
    };
    (text.encode_utf16().count() as u32 + past) as i32
}
