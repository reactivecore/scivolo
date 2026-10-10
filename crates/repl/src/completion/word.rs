//! The word under the cursor, as the REPL's JLine parser finds it (`JLineTerminal.Parser` in
//! `ParseContext.COMPLETE`), and when Tab inserts a tab instead.

use crate::commands::COMMANDS;
use std::ops::Range;
use syntax::config::Config;
use syntax::names::Names;
use syntax::scan::Scanner;
use syntax::source::SourceFile;
use syntax::token::Token;
use syntax::token::sets::{IDENTIFIER_TOKENS, KEYWORDS};

/// The byte range of the word at byte offset `cursor` of `text`: all of `text` if it starts a
/// command's name, else the identifier or keyword token around the cursor, from an unclosed
/// backtick before it; empty at the cursor if there is none.
pub fn word(text: &str, cursor: usize) -> Range<usize> {
    if COMMANDS.iter().any(|c| c.starts_with(text)) {
        return 0..text.len();
    }
    let source = SourceFile::new("<completions>", text);
    let mut names = Names::default();
    let mut s = Scanner::new(&source, &mut names, Config::default());
    let mut backtick = None;
    while s.token() != Token::Eof {
        let (start, token) = (s.offset() as usize, s.token());
        s.next_token();
        let end = s.cur.last_offset as usize;
        if start <= cursor && cursor <= end {
            let completable = IDENTIFIER_TOKENS.contains(token) || KEYWORDS.contains(token);
            return if completable { backtick.unwrap_or(start)..end } else { cursor..cursor };
        }
        backtick = (token == Token::Error && text[start..].starts_with('`')).then_some(start);
    }
    cursor..cursor
}

/// Whether Tab inserts a tab in `text`, whose last line is blank (`Option.INSERT_TAB`).
pub fn inserts_tab(text: &str) -> bool {
    let last = text.rsplit('\n').next().unwrap_or_default();
    last.chars().all(|c| matches!(c, ' ' | '\t' | '\r'))
}

#[cfg(test)]
mod tests {
    use super::{inserts_tab, word};

    fn at(text: &str, cursor: usize) -> &str {
        &text[word(text, cursor)]
    }

    #[test]
    fn finds_the_word_under_the_cursor() {
        assert_eq!(at("x.he", 4), "he");
        assert_eq!(at("x.head", 4), "head");
        assert_eq!(at("x.", 2), "");
        assert_eq!(at("(1, 2)._", 8), "_");
        assert_eq!(at(":t", 2), ":t");
        assert_eq!(at(":type x.he", 10), "he");
        assert_eq!(at("x.he\nx.size", 4), "he");
        assert_eq!(at("12", 2), "");
        assert_eq!(at("x.`hea", 6), "`hea");
    }

    #[test]
    fn spans_the_word_around_the_cursor() {
        assert_eq!(word("x.head", 4), 2..6);
        assert_eq!(word("x.", 2), 2..2);
        assert_eq!(word("12 ", 3), 3..3);
        assert_eq!(word(":lo", 1), 0..3);
    }

    #[test]
    fn inserts_a_tab_on_a_blank_line() {
        assert!(inserts_tab(""));
        assert!(inserts_tab("val x = 1\n  "));
        assert!(!inserts_tab("x."));
    }
}
