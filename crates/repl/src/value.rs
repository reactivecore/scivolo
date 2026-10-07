//! Values as the REPL prints them: `ScalaRunTime.stringOf`'s text, quoted for strings and
//! characters and suffixed for `Long` and `Float` as pprint does, without the wrapper prefix.

use crate::line_names::PREFIX as WRAPPER_PREFIX;
use tasty::Erased;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quote {
    None,
    Str,
    Char,
    Long,
    Float,
}

/// How a value of a type that erases to `erased` is quoted.
pub fn quote(erased: &Erased) -> Quote {
    match erased {
        Erased::Class(c) if c == "java.lang.String" => Quote::Str,
        Erased::Prim('c') => Quote::Char,
        Erased::Prim('j') => Quote::Long,
        Erased::Prim('f') => Quote::Float,
        _ => Quote::None,
    }
}

/// The text of a value that `ScalaRunTime.stringOf` shows as `shown`.
pub fn render(shown: &str, quote: Quote) -> String {
    match quote {
        Quote::Str if shown.contains(['\n', '\r']) => format!("\"\"\"{shown}\"\"\""),
        Quote::Str => format!("\"{}\"", shown.chars().map(escape).collect::<String>()),
        Quote::Char => format!("'{}'", shown.chars().map(escape).collect::<String>()),
        Quote::Long => format!("{shown}L"),
        Quote::Float => format!("{shown}F"),
        Quote::None => strip_wrapper(shown).to_owned(),
    }
}

/// pprint's `Util.escapeChar`, without escaping non-ASCII characters.
fn escape(c: char) -> String {
    match c {
        '"' => "\\\"".into(),
        '\\' => "\\\\".into(),
        '\u{8}' => "\\b".into(),
        '\u{c}' => "\\f".into(),
        '\n' => "\\n".into(),
        '\r' => "\\r".into(),
        '\t' => "\\t".into(),
        c if c < ' ' => format!("\\u{:04x}", c as u32),
        c => c.into(),
    }
}

/// `s` without a leading wrapper name `rs$line$N.` or `rs$line$N$` (dotty's `stripReplPrefix`).
fn strip_wrapper(s: &str) -> &str {
    match s.strip_prefix(WRAPPER_PREFIX) {
        Some(rest) => rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '$'),
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::{Quote, render};

    #[test]
    fn quotes_like_pprint() {
        assert_eq!(render("tab\there", Quote::Str), "\"tab\\there\"");
        assert_eq!(render("a\nb", Quote::Str), "\"\"\"a\nb\"\"\"");
        assert_eq!(render("c", Quote::Char), "'c'");
        assert_eq!(render("1", Quote::Long), "1L");
        assert_eq!(render("rs$line$3$C@1f", Quote::None), "C@1f");
    }
}
