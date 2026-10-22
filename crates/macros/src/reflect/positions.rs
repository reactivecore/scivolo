//! The positions of `quotes.reflect`: a position is the number of its source file and its two
//! offsets in that file.

use super::{Answer, UNKNOWN, arg};
use crate::host::Host;
use syntax::ast::Span;
use syntax::source::{Offset, SourceFile, Sources};

impl Host {
    /// Runs position operation `op` on `args`.
    pub fn run_position(&mut self, op: &str, args: &[i32], _: &str) -> Answer {
        let arg = |i: usize| arg(args, i);
        let file = |n: i32| self.sources.get(n as usize).ok_or("a position in no source file");
        let text = |s: String| Ok((Vec::new(), Some(s)));
        match op {
            "expansionPos" => self.located(self.expansion),
            "pos" => self.located(self.tpd.span(self.tree(arg(0)? as u32)?)),
            "position" => {
                let f = file(arg(0)?)?;
                let (start, end) = (arg(1)?, arg(2)?);
                if start < 0 || end < start {
                    return Err("a position with offsets out of order");
                }
                let byte = |units: i32| f.advance_utf16(0, units as usize);
                let at = |units: i32| {
                    let line = f.offset_to_line(byte(units));
                    [line as i32, units - utf16(f, f.line_to_offset(line))]
                };
                let code = f.text.get(byte(start) as usize..byte(end) as usize).map(str::to_owned);
                Ok(([at(start), at(end)].concat(), code))
            }
            "sourcePath" => text(file(arg(0)?)?.name.clone()),
            "sourceContent" => text(file(arg(0)?)?.text.clone()),
            _ => Err(UNKNOWN),
        }
    }

    /// `span` as the number of its source file and its offsets in that file, in UTF-16 units.
    fn located(&self, span: Span) -> Answer {
        if !span.exists() || span.is_in_other_source() || self.sources.is_empty() {
            return Err("a position outside the sources of the unit");
        }
        let span = span.without_inlined_call();
        let sources = Sources::new(&self.sources).map_err(|_| "sources of more than 1 GiB")?;
        let i = sources.index(span.start());
        let (base, f) = sources.files()[i];
        Ok((vec![i as i32, utf16(f, span.start() - base), utf16(f, span.end() - base)], None))
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
