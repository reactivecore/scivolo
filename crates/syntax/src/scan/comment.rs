// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/Scanners.scala
//! Line comments and nested block comments.

use super::Scanner;
use super::reader::{CR, FF, LF, SU};
use crate::source::Offset;
use std::ops::Range;

impl Scanner<'_> {
    /// After a `/`: skip a comment, record it and return true, or return false if there is none.
    pub fn skip_comment(&mut self) -> bool {
        let start = self.reader.last_char_offset;
        self.reader.next_char();
        match self.reader.ch {
            '/' => loop {
                self.reader.next_char();
                if matches!(self.reader.ch, CR | LF | SU) {
                    break;
                }
            },
            '*' => {
                self.reader.next_char();
                self.skip_block_comment();
            }
            _ => return false,
        }
        let comment = start..self.reader.ch_offset();
        self.keep_doc_comment(comment.clone());
        self.comments.push(comment);
        true
    }

    /// Keeps `comment` by the offset of the next non-whitespace character if it is a doc
    /// comment, else keeps the doc comment right before it there too (dotty's `addComment`).
    fn keep_doc_comment(&mut self, comment: Range<Offset>) {
        let text = &self.source.text;
        let after = &text[comment.end as usize..];
        let next = after.find(|c| !matches!(c, ' ' | '\t' | CR | LF | FF)).unwrap_or(after.len());
        let next = comment.end + next as Offset;
        let doc = match text[comment.start as usize..].starts_with("/**") {
            true => comment,
            false => match self.doc_comments.get(&comment.start) {
                Some(doc) => doc.clone(),
                None => return,
            },
        };
        self.doc_comments.insert(next, doc);
    }

    /// Gets the doc comment right before offset `start`, the start of a definition (dotty's
    /// `getDocComment`).
    pub fn doc_comment(&self, start: Offset) -> Option<Range<Offset>> {
        self.doc_comments.get(&start).cloned()
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
