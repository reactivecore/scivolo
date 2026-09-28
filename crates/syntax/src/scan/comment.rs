//! Line comments and nested block comments.

use super::Scanner;
use super::reader::{CR, LF, SU};

impl Scanner<'_> {
    /// After a `/`: skip a comment and return true, or return false if there is none.
    pub fn skip_comment(&mut self) -> bool {
        self.reader.next_char();
        match self.reader.ch {
            '/' => {
                loop {
                    self.reader.next_char();
                    if matches!(self.reader.ch, CR | LF | SU) {
                        break;
                    }
                }
                true
            }
            '*' => {
                self.reader.next_char();
                self.skip_block_comment();
                true
            }
            _ => false,
        }
    }

    /// Skip to the end of a block comment whose `/*` has been read, with nested comments.
    fn skip_block_comment(&mut self) {
        let mut depth = 1;
        loop {
            match self.reader.ch {
                '/' => {
                    self.reader.next_char();
                    if self.reader.ch == '*' {
                        self.reader.next_char();
                        depth += 1;
                    }
                }
                '*' => {
                    while {
                        self.reader.next_char();
                        self.reader.ch == '*'
                    } {}
                    if self.reader.ch == '/' {
                        self.reader.next_char();
                        depth -= 1;
                        if depth == 0 {
                            return;
                        }
                    }
                }
                SU => return self.incomplete_input_error("unclosed comment"),
                _ => self.reader.next_char(),
            }
        }
    }
}
