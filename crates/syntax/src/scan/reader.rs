//! A cursor over the source characters, as dotty's `CharArrayReader`.

use crate::source::Offset;

pub const LF: char = '\n';
pub const FF: char = '\u{0C}';
pub const CR: char = '\r';
/// The character the reader returns at the end of the text.
pub const SU: char = '\u{1A}';

#[derive(Clone)]
pub struct Reader<'a> {
    pub text: &'a str,
    /// The last read character.
    pub ch: char,
    /// The offset one past the last read character.
    pub char_offset: Offset,
    /// The offset before the last read character.
    pub last_char_offset: Offset,
    /// The start offset of the current line.
    pub line_start_offset: Offset,
    ch_len: Offset,
}

impl<'a> Reader<'a> {
    pub fn new(text: &'a str, start: Offset) -> Self {
        Reader {
            text,
            ch: SU,
            char_offset: start,
            last_char_offset: start,
            line_start_offset: start,
            ch_len: 1,
        }
    }

    fn read(&mut self) -> Option<char> {
        let idx = self.char_offset;
        self.last_char_offset = idx;
        let c = self.text.get(idx as usize..).and_then(|s| s.chars().next());
        let len = c.map_or(1, |c| c.len_utf8() as Offset);
        self.ch = c.unwrap_or(SU);
        self.ch_len = len;
        self.char_offset = idx + len;
        c
    }

    /// Advance one character, reducing CR LF to LF.
    pub fn next_char(&mut self) {
        if let Some(c) = self.read()
            && (c as u32) < 0x20
        {
            if c == CR && self.text.as_bytes().get(self.char_offset as usize) == Some(&b'\n') {
                self.char_offset += 1;
                self.ch = LF;
            }
            if self.ch == LF || self.ch == FF {
                self.line_start_offset = self.char_offset;
            }
        }
    }

    /// Advance one character, leaving CR LF intact and not tracking lines.
    pub fn next_raw_char(&mut self) {
        self.read();
    }

    /// The offset of the last read character (dotty's `charOffset - 1`).
    pub fn ch_offset(&self) -> Offset {
        self.char_offset - self.ch_len
    }

    /// The byte before the last read character, if any.
    pub fn byte_before_ch(&self) -> Option<u8> {
        let at = self.ch_offset().checked_sub(1)?;
        self.text.as_bytes().get(at as usize).copied()
    }

    pub fn is_at_end(&self) -> bool {
        self.char_offset as usize >= self.text.len()
    }

    /// The character after the last read one.
    pub fn lookahead_char(&self) -> char {
        let mut r = self.clone();
        r.next_char();
        r.ch
    }
}
