// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaScanners.scala
//! The tokens of a Java source: names and keywords alike as identifiers, literals, and operators
//! one character each but `...`, `::` and `->`.

/// A token of a Java source.
#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    Ident(String),
    Int(i64),
    Long(i64),
    Float(f64),
    Double(f64),
    Char(char),
    Str(String),
    Op(&'static str),
    Eof,
}

/// The operators of more than one character this scanner keeps together.
const LONG_OPS: [&str; 3] = ["...", "::", "->"];

/// The operators of one character.
const OPS: &str = "{}()[];,.@<>?=&*+-!~:|^%/";

/// Scans `text` into tokens, Unicode escapes translated first; the refusal of a construct this
/// scanner does not read.
pub fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let chars = unescaped(text)?;
    let mut s = Scanner { chars, at: 0 };
    let mut out = Vec::new();
    loop {
        let t = s.next()?;
        let end = t == Token::Eof;
        out.push(t);
        if end {
            return Ok(out);
        }
    }
}

/// `text` with its Unicode escapes (`\uXXXX` after an even number of backslashes) translated.
fn unescaped(text: &str) -> Result<Vec<char>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let (mut i, mut backslashes) = (0, 0);
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && backslashes % 2 == 0 && chars.get(i + 1) == Some(&'u') {
            let mut j = i + 1;
            while chars.get(j) == Some(&'u') {
                j += 1;
            }
            let hex: String = chars.get(j..j + 4).ok_or("a truncated Unicode escape")?.iter().collect();
            let code = u32::from_str_radix(&hex, 16).map_err(|_| format!("Unicode escape `\\u{hex}`"))?;
            out.push(char::from_u32(code).ok_or(format!("Unicode escape `\\u{hex}` of a surrogate"))?);
            i = j + 4;
            backslashes = 0;
            continue;
        }
        backslashes = if c == '\\' { backslashes + 1 } else { 0 };
        out.push(c);
        i += 1;
    }
    Ok(out)
}

struct Scanner {
    chars: Vec<char>,
    at: usize,
}

impl Scanner {
    fn peek(&self, n: usize) -> Option<char> {
        self.chars.get(self.at + n).copied()
    }

    fn starts(&self, text: &str) -> bool {
        text.chars().enumerate().all(|(i, c)| self.peek(i) == Some(c))
    }

    fn next(&mut self) -> Result<Token, String> {
        self.skip_space()?;
        let Some(c) = self.peek(0) else { return Ok(Token::Eof) };
        if c.is_alphabetic() || c == '_' || c == '$' {
            let start = self.at;
            while self.peek(0).is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$') {
                self.at += 1;
            }
            return Ok(Token::Ident(self.chars[start..self.at].iter().collect()));
        }
        if c.is_ascii_digit() || c == '.' && self.peek(1).is_some_and(|d| d.is_ascii_digit()) {
            return super::literals::number(&self.chars, &mut self.at);
        }
        if self.starts("\"\"\"") {
            self.at += 3;
            return super::text_blocks::text_block(&self.chars, &mut self.at);
        }
        if c == '"' || c == '\'' {
            return super::literals::quoted(&self.chars, &mut self.at);
        }
        if let Some(op) = LONG_OPS.into_iter().find(|op| self.starts(op)) {
            self.at += op.len();
            return Ok(Token::Op(op));
        }
        let Some(i) = OPS.find(c) else { return Err(format!("character `{c}`")) };
        self.at += 1;
        Ok(Token::Op(&OPS[i..i + 1]))
    }

    fn skip_space(&mut self) -> Result<(), String> {
        loop {
            if self.peek(0).is_some_and(char::is_whitespace) {
                self.at += 1;
            } else if self.starts("//") {
                while self.peek(0).is_some_and(|c| c != '\n') {
                    self.at += 1;
                }
            } else if self.starts("/*") {
                self.at += 2;
                while !self.starts("*/") {
                    self.peek(0).ok_or("an unclosed comment")?;
                    self.at += 1;
                }
                self.at += 2;
            } else {
                return Ok(());
            }
        }
    }
}
