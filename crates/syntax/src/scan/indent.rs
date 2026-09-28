//! Indentation widths: runs of spaces and tabs, ordered by the prefix relation.

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
    runs: Vec<Run>,
}

impl IndentWidth {
    pub fn zero() -> Self {
        IndentWidth { runs: vec![Run { ch: ' ', n: 0 }] }
    }

    /// The indentation of the line containing `offset` in `text`.
    pub fn at(text: &str, offset: u32) -> Self {
        let bytes = text.as_bytes();
        let (mut ch, mut n, mut right) = (' ', 0, Vec::new());
        for &b in bytes[..(offset as usize).min(bytes.len())].iter().rev() {
            match b {
                b'\n' => break,
                b' ' | b'\t' if b as char == ch => n += 1,
                b' ' | b'\t' => {
                    if n > 0 {
                        right.push(Run { ch, n });
                    }
                    (ch, n) = (b as char, 1);
                }
                _ => (ch, n, right) = (' ', 0, Vec::new()),
            }
        }
        right.push(Run { ch, n });
        right.reverse();
        IndentWidth { runs: right }
    }

    fn split_last(&self) -> (&[Run], Run) {
        let (last, init) = self.runs.split_last().expect("an indent width has a run");
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
        Self::le_runs(&self.runs, &that.runs)
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
        for (i, r) in self.runs.iter().enumerate() {
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

    #[test]
    fn widths_of_lines() {
        let text = "a\n  b\n\t  c\n  d e";
        let w = |o| IndentWidth::at(text, o);
        assert_eq!(w(4).to_string(), "2 spaces");
        assert_eq!(w(9).to_string(), "1 tab, 2 spaces");
        assert_eq!(w(15).to_string(), "2 spaces");
        assert!(IndentWidth::zero().lt(&w(4)));
        assert!(!w(4).le(&w(9)) && !w(9).le(&w(4)));
        assert!(w(4).is_close(&w(15)));
    }
}
