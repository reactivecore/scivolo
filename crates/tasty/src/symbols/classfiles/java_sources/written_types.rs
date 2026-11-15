// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaParsers.scala
//! Types, type parameters and parameters as a Java source writes them (dotty's `typ`,
//! `typeArgs`, `typeParams`, `formalParams`).

use super::parser::Parser;
use super::records::Header;
use super::scanner::Token;
use super::syntax::{Arg, Type, TypeParam};

/// The primitive types and `void` by their descriptor letters.
const PRIMITIVES: [(&str, char); 9] = [
    ("boolean", 'Z'),
    ("byte", 'B'),
    ("char", 'C'),
    ("short", 'S'),
    ("int", 'I'),
    ("long", 'J'),
    ("float", 'F'),
    ("double", 'D'),
    ("void", 'V'),
];

impl Parser {
    /// Reads type parameters `<T extends A & B, …>` if the cursor is at one.
    pub fn type_params(&mut self) -> Result<Vec<TypeParam>, String> {
        let mut params = Vec::new();
        if !self.eat("<") {
            return Ok(params);
        }
        loop {
            self.annotations()?;
            let name = self.ident()?;
            let mut bounds = Vec::new();
            if self.eat_ident("extends") {
                bounds.push(self.ty()?);
                while self.eat("&") {
                    bounds.push(self.ty()?);
                }
            }
            params.push(TypeParam { name, bounds });
            if !self.eat(",") {
                break;
            }
        }
        self.expect(">")?;
        Ok(params)
    }

    /// Reads a type: a primitive type, `void` or a name with type arguments, then array brackets.
    pub fn ty(&mut self) -> Result<Type, String> {
        self.annotations()?;
        let first = self.ident()?;
        let mut t = match PRIMITIVES.iter().find(|(n, _)| *n == first) {
            Some(&(_, letter)) => Type::Primitive(letter),
            None => {
                let mut parts = vec![(first, self.type_args()?)];
                while self.is(".") && matches!(self.peek_at(1), Token::Ident(_) | Token::Op("@")) {
                    self.at += 1;
                    self.annotations()?;
                    let name = self.ident()?;
                    parts.push((name, self.type_args()?));
                }
                Type::Named(parts)
            }
        };
        self.annotations()?;
        while self.is("[") && matches!(self.peek_at(1), Token::Op("]")) {
            self.at += 2;
            t = Type::Array(Box::new(t));
        }
        Ok(t)
    }

    /// Reads a parameter list or record header: the names and types, and whether the last is a
    /// varargs one (dotty's `formalParams`); the refusal of a receiver parameter `C this`.
    pub fn params(&mut self) -> Result<Header, String> {
        self.expect("(")?;
        let mut components = Vec::new();
        let mut varargs = false;
        while !self.eat(")") {
            self.modifiers()?;
            let mut t = self.ty()?;
            if self.eat("...") {
                varargs = true;
                t = Type::Array(Box::new(t));
            }
            let name = self.ident()?;
            if name == "this" {
                return Err("a receiver parameter".to_owned());
            }
            while self.eat("[") {
                self.expect("]")?;
                t = Type::Array(Box::new(t));
            }
            components.push((name, t));
            if !self.eat(",") {
                self.expect(")")?;
                break;
            }
        }
        Ok(Header { components, varargs })
    }

    fn type_args(&mut self) -> Result<Vec<Arg>, String> {
        let mut args = Vec::new();
        if !self.eat("<") {
            return Ok(args);
        }
        loop {
            self.annotations()?;
            let arg = if self.eat("?") {
                if self.eat_ident("extends") {
                    Arg::Extends(self.ty()?)
                } else if self.eat_ident("super") {
                    Arg::Super(self.ty()?)
                } else {
                    Arg::Wildcard
                }
            } else {
                Arg::Type(self.ty()?)
            };
            args.push(arg);
            if !self.eat(",") {
                break;
            }
        }
        self.expect(">")?;
        Ok(args)
    }
}
