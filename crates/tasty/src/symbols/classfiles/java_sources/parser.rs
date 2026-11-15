// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaParsers.scala
//! The parser of a Java source's declarations: the compilation unit, the cursor and modifiers;
//! annotations, types and declarations in the sibling modules.

use super::scanner::Token;
use super::syntax::{Annotation, Import, Unit};
use classfile::{ACC_ABSTRACT, ACC_FINAL, ACC_NATIVE, ACC_PRIVATE, ACC_PROTECTED, ACC_PUBLIC, ACC_STATIC};

/// The access flag the parser gives a `default` method, outside the class-file flags.
pub const DEFAULT: u16 = 0x8000;

/// The access flag the parser gives a `sealed` class, which no source sets as `ACC_SYNTHETIC`.
pub const SEALED: u16 = classfile::ACC_SYNTHETIC;

/// Parses Java source `text` into its declarations (dotty's `compilationUnit`); the refusal of a
/// construct the parser does not read, by name.
pub fn parse(text: &str) -> Result<Unit, String> {
    let tokens = super::scanner::tokens(text)?;
    let mut p = Parser { tokens, at: 0 };
    p.annotations()?;
    let mut package = String::new();
    if p.eat_ident("package") {
        package = p.qualified()?.join(".");
        p.expect(";")?;
    }
    let mut imports = Vec::new();
    while p.eat_ident("import") {
        let is_static = p.eat_ident("static");
        let mut path = vec![p.ident()?];
        let mut on_demand = false;
        while p.eat(".") {
            if p.eat("*") {
                on_demand = true;
                break;
            }
            path.push(p.ident()?);
        }
        p.expect(";")?;
        if !is_static {
            imports.push(Import { path, on_demand });
        }
    }
    let mut classes = Vec::new();
    while p.peek() != &Token::Eof {
        if p.eat(";") {
            continue;
        }
        let (access, annotations) = p.modifiers()?;
        classes.push(p.type_decl(access, annotations, false)?);
    }
    Ok(Unit { package, imports, classes })
}

/// The tokens of a source and the index of the next one.
pub struct Parser {
    pub tokens: Vec<Token>,
    pub at: usize,
}

impl Parser {
    pub fn peek(&self) -> &Token {
        &self.tokens[self.at.min(self.tokens.len() - 1)]
    }

    pub fn peek_at(&self, n: usize) -> &Token {
        &self.tokens[(self.at + n).min(self.tokens.len() - 1)]
    }

    pub fn bump(&mut self) -> Token {
        let t = self.peek().clone();
        self.at += 1;
        t
    }

    pub fn is(&self, op: &str) -> bool {
        matches!(self.peek(), Token::Op(o) if *o == op)
    }

    pub fn is_ident(&self, name: &str) -> bool {
        matches!(self.peek(), Token::Ident(n) if n == name)
    }

    pub fn eat(&mut self, op: &str) -> bool {
        let found = self.is(op);
        self.at += usize::from(found);
        found
    }

    pub fn eat_ident(&mut self, name: &str) -> bool {
        let found = self.is_ident(name);
        self.at += usize::from(found);
        found
    }

    pub fn expect(&mut self, op: &str) -> Result<(), String> {
        if self.eat(op) { Ok(()) } else { Err(format!("`{op}` expected, {:?} found", self.peek())) }
    }

    pub fn ident(&mut self) -> Result<String, String> {
        match self.bump() {
            Token::Ident(n) => Ok(n),
            t => Err(format!("a name expected, {t:?} found")),
        }
    }

    pub fn qualified(&mut self) -> Result<Vec<String>, String> {
        let mut path = vec![self.ident()?];
        while self.eat(".") {
            path.push(self.ident()?);
        }
        Ok(path)
    }

    /// Skips the tokens up to the `close` matching the `open` just read.
    pub fn skip_balanced(&mut self, open: &str, close: &str) -> Result<(), String> {
        let mut depth = 1;
        while depth > 0 {
            match self.bump() {
                Token::Eof => return Err(format!("an unclosed `{open}`")),
                Token::Op(o) if o == open => depth += 1,
                Token::Op(o) if o == close => depth -= 1,
                _ => {}
            }
        }
        Ok(())
    }

    /// Skips to the next `,` or `;` outside parentheses and braces (dotty's `skipTo`).
    pub fn skip_to_comma_or_semi(&mut self) -> Result<(), String> {
        while !self.is(",") && !self.is(";") {
            match self.bump() {
                Token::Eof => return Err("`;` expected".to_owned()),
                Token::Op("{") => self.skip_balanced("{", "}")?,
                Token::Op("(") => self.skip_balanced("(", ")")?,
                _ => {}
            }
        }
        Ok(())
    }

    /// Reads modifiers and annotations: the class-file access flags with `DEFAULT` and `SEALED`,
    /// and the annotations; `non-sealed` is skipped.
    pub fn modifiers(&mut self) -> Result<(u16, Vec<Annotation>), String> {
        let mut access = 0;
        let mut annotations = Vec::new();
        loop {
            if self.at_annotation() {
                annotations.push(self.annotation()?);
                continue;
            }
            let Token::Ident(word) = self.peek() else { break };
            access |= match word.as_str() {
                "public" => ACC_PUBLIC,
                "protected" => ACC_PROTECTED,
                "private" => ACC_PRIVATE,
                "static" => ACC_STATIC,
                "abstract" => ACC_ABSTRACT,
                "final" => ACC_FINAL,
                "native" => ACC_NATIVE,
                "default" if !matches!(self.peek_at(1), Token::Op(":")) => DEFAULT,
                "synchronized" | "transient" | "volatile" | "strictfp" => 0,
                "sealed" if matches!(self.peek_at(1), Token::Ident(_)) => SEALED,
                "non" if self.peek_at(1) == &Token::Op("-") => {
                    self.at += 2;
                    0
                }
                _ => break,
            };
            self.at += 1;
        }
        Ok((access, annotations))
    }
}
