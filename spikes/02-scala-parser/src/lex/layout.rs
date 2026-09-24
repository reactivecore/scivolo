use super::{Lexer, Region, Tok, Token, UNSET};

const OPENER_OPS: &[&str] = &["=", "=>", "?=>", "<-", ":", "⇒", "←"];
const OPENER_KEYWORDS: &[&str] = &[
    "then", "else", "do", "try", "catch", "finally", "yield", "match", "with", "return", "throw", "while",
    "if", "for",
];

impl Lexer<'_> {
    fn last_text(&self) -> &[u8] {
        &self.src[self.last_span.0..self.last_span.1]
    }

    fn opens_indent(&self) -> bool {
        let text = self.last_text();
        match self.last {
            Tok::Op => OPENER_OPS.iter().any(|o| o.as_bytes() == text),
            Tok::Keyword => OPENER_KEYWORDS.iter().any(|k| k.as_bytes() == text),
            _ => false,
        }
    }

    fn ends_statement(&self) -> bool {
        !matches!(self.last, Tok::Open | Tok::Comma | Tok::Dot | Tok::Newline | Tok::Semi)
            && self.peek(0) != b'.'
    }

    fn marker(&self, kind: Tok) -> Token {
        Token { kind, start: self.pos as u32, end: self.pos as u32 }
    }

    pub(super) fn layout(&mut self) -> Option<Token> {
        if self.pos >= self.src.len() {
            return None;
        }
        let col = (self.pos - self.line_start) as u32;
        let current = match self.regions.last_mut()? {
            Region::Brace(c) if *c == UNSET => {
                *c = col;
                return None;
            }
            Region::Indent(c) | Region::Brace(c) => *c,
            _ => return None,
        };
        if col > current {
            if !self.opens_indent() {
                return None;
            }
            self.regions.push(Region::Indent(col));
            return Some(self.marker(Tok::Indent));
        }
        let mut queued = Vec::new();
        while let Some(&Region::Indent(c)) = self.regions.last()
            && c > col
            && self.regions.len() > 1
        {
            self.regions.pop();
            queued.push(self.marker(Tok::Outdent));
        }
        if self.ends_statement() {
            queued.push(self.marker(Tok::Newline));
        }
        let first = (!queued.is_empty()).then(|| queued.remove(0));
        self.pending.extend(queued.into_iter().rev());
        first
    }
}
