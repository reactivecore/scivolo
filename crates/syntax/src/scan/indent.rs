//! Indentation widths: runs of spaces and tabs, ordered by the prefix relation.

use crate::source::SourceFile;
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Run {
    ch: char,
    n: u32,
}

impl Run {
    fn le(self, that: Run) -> bool {
        self.n <= that.n && (self.ch == that.ch || self.n == 0)
    }
}

/// The runs of a line's leading whitespace, left to right.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct IndentWidth {
    runs: Runs,
}

/// Up to `INLINE` runs in place, more on the heap.
#[derive(Clone, Debug)]
enum Runs {
    Inline([Run; INLINE], u8),
    Heap(Box<[Run]>),
}

const INLINE: usize = 3;

impl Runs {
    fn as_slice(&self) -> &[Run] {
        match self {
            Runs::Inline(runs, len) => &runs[..*len as usize],
            Runs::Heap(runs) => runs,
        }
    }
}

impl PartialEq for Runs {
    fn eq(&self, other: &Runs) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for Runs {}

impl IndentWidth {
    pub fn zero() -> Self {
        Self::of(&[Run { ch: ' ', n: 0 }])
    }

    fn of(runs: &[Run]) -> Self {
        let runs = if runs.len() <= INLINE {
            let mut inline = [Run { ch: ' ', n: 0 }; INLINE];
            inline[..runs.len()].copy_from_slice(runs);
            Runs::Inline(inline, runs.len() as u8)
        } else {
            Runs::Heap(runs.into())
        };
        IndentWidth { runs }
    }

    /// The leading blanks, before `offset`, of the line (ended by `\n`) containing `offset`.
    pub fn at(source: &SourceFile, offset: u32) -> Self {
        let bytes = source.text.as_bytes();
        let line = &bytes[source.after_last_line_feed(offset) as usize..(offset as usize).min(bytes.len())];
        let blanks = &line[..line.iter().position(|&b| b != b' ' && b != b'\t').unwrap_or(line.len())];
        let run = |chunk: &[u8]| Run { ch: chunk[0] as char, n: chunk.len() as u32 };
        let count = blanks.chunk_by(|a, b| a == b).count();
        if count == 0 {
            return Self::zero();
        }
        if count > INLINE {
            return IndentWidth { runs: Runs::Heap(blanks.chunk_by(|a, b| a == b).map(run).collect()) };
        }
        let mut inline = [Run { ch: ' ', n: 0 }; INLINE];
        for (slot, chunk) in inline.iter_mut().zip(blanks.chunk_by(|a, b| a == b)) {
            *slot = run(chunk);
        }
        IndentWidth { runs: Runs::Inline(inline, count as u8) }
    }

    fn split_last(&self) -> (&[Run], Run) {
        let (last, init) = self.runs.as_slice().split_last().expect("an indent width has a run");
        (init, *last)
    }

    fn le_runs(a: &[Run], b: &[Run]) -> bool {
        match (a, b) {
            ([x], [y]) => x.le(*y),
            ([_], [.., _]) => Self::le_runs(a, &b[..b.len() - 1]),
            ([.., x], [.., y]) if b.len() > 1 => a[..a.len() - 1] == b[..b.len() - 1] && x.le(*y),
            _ => false,
        }
    }

    pub fn le(&self, that: &IndentWidth) -> bool {
        Self::le_runs(self.runs.as_slice(), that.runs.as_slice())
    }

    pub fn lt(&self, that: &IndentWidth) -> bool {
        self.le(that) && !that.le(self)
    }

    /// Whether `self` differs from `that` by at most one space.
    pub fn is_close(&self, that: &IndentWidth) -> bool {
        let ((i1, r1), (i2, r2)) = (self.split_last(), that.split_last());
        i1 == i2 && r1.ch == r2.ch && r1.ch != '\t' && r1.n.abs_diff(r2.n) <= 1
    }
}

impl fmt::Display for IndentWidth {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (i, r) in self.runs.as_slice().iter().enumerate() {
            let kind = match r.ch {
                ' ' => "space".to_string(),
                '\t' => "tab".to_string(),
                c => format!("'{c}'-character"),
            };
            let sep = if i == 0 { "" } else { ", " };
            write!(f, "{sep}{} {kind}{}", r.n, if r.n == 1 { "" } else { "s" })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::IndentWidth;
    use crate::source::SourceFile;

    #[test]
    fn widths_of_lines() {
        let text = SourceFile::new("t", "a\n  b\n\t  c\n  d e");
        let w = |o| IndentWidth::at(&text, o);
        assert_eq!(w(4).to_string(), "2 spaces");
        assert_eq!(w(9).to_string(), "1 tab, 2 spaces");
        assert_eq!(w(15).to_string(), "2 spaces");
        assert!(IndentWidth::zero().lt(&w(4)));
        assert!(!w(4).le(&w(9)) && !w(9).le(&w(4)));
        assert!(w(4).is_close(&w(15)));
    }

    #[test]
    fn widths_count_leading_blanks_only_and_keep_many_runs() {
        let text = SourceFile::new("t", "x\n \t \t y z\n \t \t w");
        let w = |o| IndentWidth::at(&text, o);
        assert_eq!(w(9).to_string(), "1 space, 1 tab, 1 space, 1 tab, 1 space");
        assert_eq!(w(9), w(18));
        assert!(!w(9).lt(&w(18)));
        assert_eq!(w(4).to_string(), "1 space, 1 tab");
    }

    #[test]
    fn only_line_feeds_end_lines() {
        let text = SourceFile::new("t", "x\n  a\r  b");
        assert_eq!(IndentWidth::at(&text, 8).to_string(), "2 spaces");
    }
}
