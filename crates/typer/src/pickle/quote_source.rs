use super::Refusal;
use syntax::ast::Span;
use syntax::source::{Offset, SourceFile};
use tasty::TastySpan;
use tasty::write::text_line_sizes;

/// The source of a quote: the path of its root's file from the source root and the size of each
/// line of the file it is pickled in.
pub struct QuoteSource<'a> {
    pub path: String,
    pub line_sizes: Vec<u32>,
    /// The file the quote is pickled in, at its offset among the sources; without it, the spans
    /// are read from the quote's TASTy and count in its UTF-16 units already.
    pub file: Option<(&'a SourceFile, Offset)>,
    /// The other source files of the quote's unit, each at its offset among the sources.
    pub others: Vec<(&'a SourceFile, Offset)>,
}

impl<'a> QuoteSource<'a> {
    /// The source `file` at offset `base` among the sources, the path of the quote's root `path`.
    pub fn of(path: String, file: &'a SourceFile, base: Offset) -> QuoteSource<'a> {
        let line_sizes = text_line_sizes(&file.text);
        QuoteSource { path, line_sizes, file: Some((file, base)), others: Vec::new() }
    }

    /// The path of the file of the unit that `span` starts in, if it starts in one.
    pub fn unit_file(&self, span: Span) -> Option<String> {
        if !span.exists() || span.without_inlined_call().is_in_other_source() {
            return None;
        }
        let start = span.without_inlined_call().in_own_source().start();
        let (file, _) = self.files().find(|&(f, base)| within(f, base, start))?;
        Some(file.tasty_path().to_owned())
    }

    /// The TASTy span of `span`, in a library's source only if `library` is known.
    pub fn tasty_span(&self, span: Span, library: bool) -> Result<TastySpan, Refusal> {
        let span = span.without_inlined_call();
        let own = !span.is_in_other_source();
        if !own && self.file.is_some() && !library {
            return Err("a quote with a span in a library's source");
        }
        let span = span.in_own_source();
        let point = (!span.is_synthetic()).then(|| self.units(span.point(), own)).transpose()?;
        Ok(TastySpan { start: self.units(span.start(), own)?, end: self.units(span.end(), own)?, point })
    }

    /// The files of the unit, each at its offset among the sources, the quote's first.
    fn files(&self) -> impl Iterator<Item = (&'a SourceFile, Offset)> + '_ {
        self.file.into_iter().chain(self.others.iter().copied())
    }

    /// Offset `o` of a span in UTF-16 units of the file of the unit it is in.
    fn units(&self, o: Offset, own: bool) -> Result<u32, Refusal> {
        if !own {
            return Ok(o);
        }
        self.file.ok_or("a span of the quote's source without the source")?;
        let in_unit = self.files().find(|&(f, b)| within(f, b, o));
        let text = in_unit.and_then(|(file, base)| file.text.get(..(o - base) as usize));
        let text = text.ok_or("a quote with a span in another of the sources")?;
        Ok(if text.is_ascii() { text.len() as u32 } else { text.encode_utf16().count() as u32 })
    }
}

/// Whether offset `o` lies in `file` at offset `base`, its end included.
fn within(file: &SourceFile, base: Offset, o: Offset) -> bool {
    o >= base && o - base <= file.text.len() as Offset
}

#[cfg(test)]
mod tests {
    use super::{QuoteSource, SourceFile, Span};

    #[test]
    fn spans_in_other_files_of_the_unit_count_in_their_file() {
        let (a, b) = (SourceFile::new("a.scala", "object A"), SourceFile::new("b.scala", "val ü = 1"));
        let mut source = QuoteSource::of("b.scala".into(), &a, 0);
        source.others = vec![(&b, 9)];
        assert_eq!(source.line_sizes, [8]);
        assert_eq!(source.unit_file(Span::new(0, 8, 0)).as_deref(), Some("a.scala"));
        assert_eq!(source.unit_file(Span::new(8, 8, 8)).as_deref(), Some("a.scala"));
        assert_eq!(source.unit_file(Span::new(9, 19, 13)).as_deref(), Some("b.scala"));
        let span = source.tasty_span(Span::new(13, 19, 13), false).unwrap();
        assert_eq!((span.start, span.end, span.point), (4, 9, Some(4)));
    }
}
