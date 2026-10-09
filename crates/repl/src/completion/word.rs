//! The word under the cursor, as the REPL's JLine parser finds it (`JLineTerminal.Parser` in
//! `ParseContext.COMPLETE`), and when Tab inserts a tab instead.

use crate::commands::COMMANDS;
use syntax::config::Config;
use syntax::names::Names;
use syntax::scan::Scanner;
use syntax::source::SourceFile;
use syntax::token::Token;
use syntax::token::sets::{IDENTIFIER_TOKENS, KEYWORDS};

/// The word at byte offset `cursor` of `text`: all of `text` if it starts a command's name,
/// else the identifier or keyword token around the cursor, from an unclosed backtick before it;
/// empty if there is none.
pub fn word(text: &str, cursor: usize) -> &str {
    if COMMANDS.iter().any(|c| c.starts_with(text)) {
        return text;
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
            return if completable { &text[backtick.unwrap_or(start)..end] } else { "" };
        }
        backtick = (token == Token::Error && text[start..].starts_with('`')).then_some(start);
    }
    ""
}

/// Whether Tab inserts a tab in `text`, whose last line is blank (`Option.INSERT_TAB`).
pub fn inserts_tab(text: &str) -> bool {
    let last = text.rsplit('\n').next().unwrap_or_default();
    last.chars().all(|c| matches!(c, ' ' | '\t' | '\r'))
}

#[cfg(test)]
mod tests {
    use super::{inserts_tab, word};

    #[test]
    fn finds_the_word_under_the_cursor() {
        assert_eq!(word("x.he", 4), "he");
        assert_eq!(word("x.head", 4), "head");
        assert_eq!(word("x.", 2), "");
        assert_eq!(word("(1, 2)._", 8), "_");
        assert_eq!(word(":t", 2), ":t");
        assert_eq!(word(":type x.he", 10), "he");
        assert_eq!(word("x.he\nx.size", 4), "he");
        assert_eq!(word("12", 2), "");
    }

    #[test]
    fn inserts_a_tab_on_a_blank_line() {
        assert!(inserts_tab(""));
        assert!(inserts_tab("val x = 1\n  "));
        assert!(!inserts_tab("x."));
    }
}
