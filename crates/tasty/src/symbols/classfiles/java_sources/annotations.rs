// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaParsers.scala
//! Annotations of a Java source (dotty's `annotation`): the name and the enum constants of the
//! arguments, the other arguments skipped.

use super::parser::Parser;
use super::scanner::Token;
use super::syntax::Annotation;

impl Parser {
    /// Determines whether the cursor is at an annotation, not at `@interface`.
    pub fn at_annotation(&self) -> bool {
        self.is("@") && !matches!(self.peek_at(1), Token::Ident(n) if n == "interface")
    }

    /// Skips the annotations at the cursor.
    pub fn annotations(&mut self) -> Result<(), String> {
        while self.at_annotation() {
            self.annotation()?;
        }
        Ok(())
    }

    /// Reads an annotation: its name and the enum constants among its arguments, `X.C` or `C`
    /// alone as an element's value.
    pub fn annotation(&mut self) -> Result<Annotation, String> {
        self.expect("@")?;
        let name = self.qualified()?;
        let mut enum_elements = Vec::new();
        if self.eat("(") {
            loop {
                let named =
                    matches!(self.peek(), Token::Ident(_)) && matches!(self.peek_at(1), Token::Op("="));
                let element = if named { self.ident()? } else { "value".to_owned() };
                if named {
                    self.expect("=")?;
                }
                let start = self.at;
                while !self.is(",") && !self.is(")") {
                    match self.bump() {
                        Token::Eof => return Err("an unclosed annotation".to_owned()),
                        Token::Op("(") => self.skip_balanced("(", ")")?,
                        Token::Op("{") => self.skip_balanced("{", "}")?,
                        _ => {}
                    }
                }
                if let Some(Token::Ident(c)) = self.enum_constant(start) {
                    enum_elements.push((element, c.clone()));
                }
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(")")?;
        }
        Ok(Annotation { name, enum_elements })
    }

    /// The last name of the tokens from `start` to the cursor if they are `X.C` or `C`.
    fn enum_constant(&self, start: usize) -> Option<&Token> {
        let value = &self.tokens[start..self.at];
        let names = value.iter().step_by(2).all(|t| matches!(t, Token::Ident(_)));
        let dots = value.iter().skip(1).step_by(2).all(|t| *t == Token::Op("."));
        (value.len() % 2 == 1 && value.len() <= 3 && names && dots).then(|| &value[value.len() - 1])
    }
}
