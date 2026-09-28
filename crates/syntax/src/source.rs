//! A source file: its text and the line index dotty's `SourceFile` computes.

/// Byte offset into a source text.
pub type Offset = u32;

pub struct SourceFile {
    pub name: String,
    pub text: String,
    /// The offset of every line start, then the text length as a sentinel.
    line_starts: Vec<Offset>,
}

impl SourceFile {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let bytes = text.as_bytes();
        let mut line_starts = vec![0];
        for (i, &b) in bytes.iter().enumerate() {
            let line_break = match b {
                b'\r' => bytes.get(i + 1) != Some(&b'\n'),
                b'\n' | 0x0C | 0x1A => true,
                _ => false,
            };
            if line_break {
                line_starts.push(i as Offset + 1);
            }
        }
        line_starts.push(text.len() as Offset);
        Self { name: name.into(), text, line_starts }
    }

    pub fn len(&self) -> Offset {
        self.text.len() as Offset
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The 0-based line of `offset`, as dotty's `offsetToLine`.
    pub fn offset_to_line(&self, offset: Offset) -> usize {
        let line = self.line_starts.partition_point(|&s| s <= offset).saturating_sub(1);
        if offset >= self.len() { line.saturating_sub(1) } else { line }
    }

    /// The offset of the first character of `line`.
    pub fn line_to_offset(&self, line: usize) -> Offset {
        self.line_starts[line]
    }

    /// The offset `units` UTF-16 units after `from`, as dotty's offsets count.
    pub fn advance_utf16(&self, from: Offset, units: usize) -> Offset {
        let rest = self.text.get(from as usize..).unwrap_or("");
        let (mut n, mut bytes) = (0, 0);
        for c in rest.chars() {
            if n >= units {
                break;
            }
            n += c.len_utf16();
            bytes += c.len_utf8();
        }
        from + bytes as Offset + (units.saturating_sub(n)) as Offset
    }

    /// The offset of the start of the line containing `offset`.
    pub fn start_of_line(&self, offset: Offset) -> Offset {
        self.line_to_offset(self.offset_to_line(offset))
    }

    /// The 0-based column of `offset` in its line, in characters.
    pub fn column(&self, offset: Offset) -> usize {
        let start = self.line_to_offset(self.offset_to_line(offset)) as usize;
        let end = (offset as usize).min(self.text.len());
        self.text.get(start..end).map_or(0, |s| s.chars().count())
    }
}

#[cfg(test)]
mod tests {
    use super::SourceFile;

    #[test]
    fn lines_follow_dotty() {
        let s = SourceFile::new("t", "a\r\nb\rc\nd");
        assert_eq!((0..=8).map(|o| s.offset_to_line(o)).collect::<Vec<_>>(), [0, 0, 0, 1, 1, 2, 2, 3, 3]);
        assert_eq!(s.column(3), 0);
    }
}
