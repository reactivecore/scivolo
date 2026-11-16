// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaParsers.scala
//! Annotations of a Java source (dotty's `annotation`): the name, the arguments and the enum
//! constants among them.

use super::parser::Parser;
use super::scanner::Token;
use super::syntax::{Annotation, ArgValue};
use classfile::ElementValue;

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

    /// Reads an annotation: its name, its arguments by element, none if one is a value dotty's
    /// parser drops the annotation for, and the enum constants among them, a name `p.X.C` or `C`
    /// alone as an element's value.
    pub fn annotation(&mut self) -> Result<Annotation, String> {
        self.expect("@")?;
        let name = self.qualified()?;
        let mut enum_elements = Vec::new();
        let mut args = Some(Vec::new());
        if self.eat("(") && !self.eat(")") {
            loop {
                let named =
                    matches!(self.peek(), Token::Ident(_)) && matches!(self.peek_at(1), Token::Op("="));
                let element = if named { self.ident()? } else { "value".to_owned() };
                if named {
                    self.expect("=")?;
                }
                let value = self.arg_value()?;
                if let Some(ArgValue::Path(path)) = &value {
                    enum_elements.push((element.clone(), path[path.len() - 1].clone()));
                }
                match (value, &mut args) {
                    (Some(v), Some(args)) => args.push((element, v)),
                    _ => args = None,
                }
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(")")?;
        }
        Ok(Annotation { name, enum_elements, args })
    }

    /// Reads an element value (dotty's `argValue`): none, skipped to the next `,`, `}` or `)`,
    /// if it is an expression of another form.
    fn arg_value(&mut self) -> Result<Option<ArgValue>, String> {
        let value = match self.peek() {
            Token::Op("@") => Some(self.annotation()?).filter(|a| a.args.is_some()).map(ArgValue::Annotation),
            Token::Op("{") => self.array()?,
            _ => match self.constant() {
                Some(c) => Some(ArgValue::Literal(c)),
                None if matches!(self.peek(), Token::Ident(_)) => {
                    let path = self.qualified()?;
                    let class = self.is_ident("class") && self.tokens[self.at - 1] == Token::Op(".");
                    Some(if class { ArgValue::ClassLiteral } else { ArgValue::Path(path) })
                }
                None => None,
            },
        };
        if self.is(",") || self.is("}") || self.is(")") {
            return Ok(value);
        }
        while !self.is(",") && !self.is("}") && !self.is(")") {
            match self.bump() {
                Token::Eof => return Err("an unclosed annotation".to_owned()),
                Token::Op("(") => self.skip_balanced("(", ")")?,
                Token::Op("{") => self.skip_balanced("{", "}")?,
                _ => {}
            }
        }
        Ok(None)
    }

    /// Reads an array value `{a, b}`; none if an element is a value dotty's parser drops.
    fn array(&mut self) -> Result<Option<ArgValue>, String> {
        self.expect("{")?;
        let mut elements = Some(Vec::new());
        while !self.eat("}") {
            match (self.arg_value()?, &mut elements) {
                (Some(v), Some(es)) => es.push(v),
                _ => elements = None,
            }
            self.eat(",");
        }
        Ok(elements.map(ArgValue::Array))
    }

    /// Reads a literal, `-` or `!` before it negating it (dotty's `tryConstant`); none, the
    /// cursor unmoved, at another token.
    fn constant(&mut self) -> Option<ElementValue> {
        let negate = self.is("-") || self.is("!");
        let literal = self.peek_at(usize::from(negate)).clone();
        let sign = |v: i64| if negate { v.wrapping_neg() } else { v };
        let value = match literal {
            Token::Ident(w) if w == "true" || w == "false" => ElementValue::Boolean((w == "true") != negate),
            Token::Char(c) => ElementValue::Char(u16::try_from(u32::from(c)).ok()?),
            Token::Int(v) => ElementValue::Int(i32::try_from(sign(v)).ok()?),
            Token::Long(v) => ElementValue::Long(sign(v)),
            Token::Float(v) => ElementValue::Float((if negate { -v } else { v }) as f32),
            Token::Double(v) => ElementValue::Double(if negate { -v } else { v }),
            Token::Str(s) => ElementValue::String(s),
            _ => return None,
        };
        self.at += 1 + usize::from(negate);
        Some(value)
    }
}
