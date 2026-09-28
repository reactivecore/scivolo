//! The scanner on small inputs; `just syntax-report` compares it with dotty on its test suites.

use syntax::config::Config;
use syntax::names::Names;
use syntax::scan::Scanner;
use syntax::source::SourceFile;
use syntax::token::Token::{self, *};

/// The tokens of `text` up to EOF, with the identifier names and literal values.
fn scan(text: &str) -> (Vec<(Token, String)>, Vec<String>) {
    scan_with(text, Config::default())
}

fn scan_with(text: &str, cfg: Config) -> (Vec<(Token, String)>, Vec<String>) {
    let source = SourceFile::new("t.scala", text);
    let mut names = Names::default();
    let mut s = Scanner::new(&source, &mut names, cfg);
    let mut out = Vec::new();
    while s.token() != Eof {
        let v = match s.cur.str_val {
            Some(_) if s.token() != Identifier => String::from_utf16_lossy(s.str_val(&s.cur).unwrap()),
            _ => s.name_text().to_string(),
        };
        out.push((s.token(), v));
        s.next_token();
    }
    (out, s.diags.items.iter().map(|d| format!("{}: {}", d.offset, d.message)).collect())
}

fn kinds(text: &str) -> Vec<Token> {
    scan(text).0.into_iter().map(|(t, _)| t).collect()
}

#[test]
fn indentation_inserts_indent_outdent_and_newlines() {
    let text = "def f =\n  val x = 1\n\n  x\nval y = 2\n";
    assert_eq!(
        kinds(text),
        [
            Def, Identifier, Equals, Indent, Val, Identifier, Equals, IntLit, Newlines, Identifier, Outdent,
            Newline, Val, Identifier, Equals, IntLit
        ]
    );
}

#[test]
fn braces_disable_indentation() {
    assert_eq!(kinds("f(\n  a,\n  b)"), [Identifier, LParen, Identifier, Comma, Identifier, RParen]);
}

#[test]
fn leading_infix_operator_continues_the_line() {
    assert_eq!(kinds("val x = a\n  + b\n"), [Val, Identifier, Equals, Identifier, Identifier, Identifier]);
}

#[test]
fn interpolated_strings_split_into_parts() {
    let (tokens, errors) = scan(r#"s"a$b${c}d""#);
    assert_eq!(
        tokens,
        [
            (InterpolationId, "s".into()),
            (StringPart, "a".into()),
            (Identifier, "b".into()),
            (StringPart, "".into()),
            (LBrace, "".into()),
            (Identifier, "c".into()),
            (RBrace, "".into()),
            (StringLit, "d".into()),
        ]
    );
    assert!(errors.is_empty());
}

#[test]
fn literals_keep_their_digits_and_escapes() {
    let (tokens, _) = scan("1_000 0x1F 2L 3.5e-2f 'a' '\\n' \"\"\"raw\\n\"\"\"");
    let values: Vec<_> = tokens.iter().map(|(t, v)| (*t, v.as_str())).collect();
    assert_eq!(
        values,
        [
            (IntLit, "1_000"),
            (IntLit, "1F"),
            (LongLit, "2"),
            (FloatLit, "3.5e-2f"),
            (CharLit, "a"),
            (CharLit, "\n"),
            (StringLit, "raw\\n")
        ]
    );
}

#[test]
fn end_markers_and_colons() {
    assert_eq!(
        kinds("object A:\n  def f = 1\nend A\nval end = 2"),
        [
            Object,
            Identifier,
            ColonFollow,
            Def,
            Identifier,
            Equals,
            IntLit,
            Newline,
            End,
            Identifier,
            Newline,
            Val,
            Identifier,
            Equals,
            IntLit
        ]
    );
}

#[test]
fn the_parser_turns_a_colon_at_line_end_into_an_indented_block() {
    let source = SourceFile::new("t.scala", "object A:\n  def f = 1\n");
    let mut names = Names::default();
    let mut s = Scanner::new(&source, &mut names, Config::default());
    s.next_token();
    s.next_token();
    s.observe_colon_eol(true);
    assert_eq!(s.token(), ColonEol);
    let mut tokens = Vec::new();
    while s.token() != Eof {
        s.next_token();
        tokens.push(s.token());
    }
    assert_eq!(tokens, [Indent, Def, Identifier, Equals, IntLit, Outdent, Eof]);
}

#[test]
fn case_class_fuses_and_comments_are_skipped() {
    assert_eq!(
        kinds("case class /* a /* nested */ b */ C // x\ncase object O"),
        [CaseClass, Identifier, Newline, CaseObject, Identifier]
    );
}

#[test]
fn errors_point_at_the_offending_input() {
    let (_, errors) = scan("val s = \"abc\nval c = '' val n = 1_ /* x");
    assert_eq!(
        errors,
        [
            "8: unclosed string literal",
            "21: empty character literal",
            "33: trailing separator is not allowed",
            "33: trailing separator is not allowed",
            "35: unclosed comment",
        ]
    );
}

#[test]
fn no_indent_keeps_newlines_only() {
    let cfg = Config::from_options(&["-no-indent"]);
    let tokens: Vec<_> = scan_with("def f =\n  1\n", cfg).0.into_iter().map(|(t, _)| t).collect();
    assert_eq!(tokens, [Def, Identifier, Equals, IntLit]);
}
