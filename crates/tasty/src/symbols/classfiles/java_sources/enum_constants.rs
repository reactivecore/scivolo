// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaParsers.scala
//! The constants of a Java enum and the members javac adds for them (dotty's `enumDecl`).

use super::parser::Parser;
use super::scanner::Token;
use super::syntax::{Class, Field, Method, Type};
use classfile::{ACC_ENUM, ACC_FINAL, ACC_PUBLIC, ACC_STATIC};

/// The parameter name of an enum's `valueOf`.
const VALUE_OF_PARAM: &str = "x";

impl Parser {
    /// Reads the constants of enum `class` up to the `;` or `}` after them, and enters them as
    /// its static fields with its `values` and `valueOf`; a constant with a body makes the enum
    /// not final.
    pub fn enum_constants(&mut self, class: &mut Class) -> Result<(), String> {
        let enum_type = Type::Named(vec![(class.name.clone(), Vec::new())]);
        let mut bodies = false;
        while let Token::Ident(_) | Token::Op("@") = self.peek() {
            self.annotations()?;
            let name = self.ident()?;
            if self.eat("(") {
                self.skip_balanced("(", ")")?;
            }
            if self.eat("{") {
                bodies = true;
                self.skip_balanced("{", "}")?;
            }
            let access = ACC_PUBLIC | ACC_STATIC | ACC_FINAL | ACC_ENUM;
            let ty = enum_type.clone();
            class.fields.push(Field { access, name, ty, constant: None, annotations: None });
            if !self.eat(",") {
                break;
            }
        }
        if !self.is("}") {
            self.expect(";")?;
        }
        class.access |= if bodies { 0 } else { ACC_FINAL };
        let array = Type::Array(Box::new(enum_type.clone()));
        let string = Type::Named(["java", "lang", "String"].map(|n| (n.to_owned(), Vec::new())).to_vec());
        let value_of = vec![(VALUE_OF_PARAM.to_owned(), string)];
        for (name, params, result) in [("values", vec![], array), ("valueOf", value_of, enum_type)] {
            let (access, name, type_params) = (ACC_PUBLIC | ACC_STATIC, name.to_owned(), Vec::new());
            class.methods.push(Method {
                access,
                name,
                type_params,
                params,
                result,
                annotation_default: false,
                annotations: Some(Vec::new()),
            });
        }
        Ok(())
    }
}
