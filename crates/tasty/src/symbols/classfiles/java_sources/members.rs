// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaParsers.scala
//! The type declarations of a Java source and their members (dotty's `typeDecl`, `typeBody`,
//! `memberDecl`, `enumDecl`, `annotationDecl`); bodies and initializers skipped.

use super::parser::{DEFAULT, Parser};
use super::scanner::Token;
use super::syntax::{Annotation, Class, ClassKind, Method, Type};
use classfile::{
    ACC_ABSTRACT, ACC_ANNOTATION, ACC_ENUM, ACC_FINAL, ACC_INTERFACE, ACC_PRIVATE, ACC_PROTECTED, ACC_PUBLIC,
    ACC_STATIC, ACC_VARARGS,
};
use names::nme;

/// The access flags that say who sees a declaration.
const VISIBILITY: u16 = ACC_PUBLIC | ACC_PROTECTED | ACC_PRIVATE;

/// The words that start a nested type declaration after its modifiers.
const TYPE_KEYWORDS: [&str; 4] = ["class", "interface", "enum", "record"];

impl Parser {
    /// Reads a class, interface, enum or annotation type of modifiers `access`, nested in an
    /// interface if `in_interface`; records are refused.
    pub fn type_decl(
        &mut self,
        access: u16,
        annotations: Vec<Annotation>,
        in_interface: bool,
    ) -> Result<Class, String> {
        let mut access = access & !DEFAULT;
        if in_interface {
            access |= ACC_PUBLIC | ACC_STATIC;
        }
        let kind = if self.eat("@") {
            access |= ACC_ANNOTATION | ACC_INTERFACE | ACC_ABSTRACT | ACC_STATIC;
            ClassKind::Annotation
        } else {
            match self.ident()?.as_str() {
                "class" => ClassKind::Class,
                "interface" => {
                    access |= ACC_INTERFACE | ACC_ABSTRACT | ACC_STATIC;
                    ClassKind::Interface
                }
                "enum" => {
                    access |= ACC_ENUM | ACC_STATIC;
                    ClassKind::Enum
                }
                "record" => return Err("a Java record".to_owned()),
                w => return Err(format!("a class declaration expected, `{w}` found")),
            }
        };
        if kind == ClassKind::Annotation {
            self.ident()?;
        }
        let name = self.ident()?;
        let type_params = self.type_params()?;
        let mut extends = None;
        let mut implements = Vec::new();
        if self.eat_ident("extends") {
            if kind == ClassKind::Interface {
                implements = self.types()?;
            } else {
                extends = Some(self.ty()?);
            }
        }
        if self.eat_ident("implements") {
            implements = self.types()?;
        }
        if self.is_ident("permits") {
            return Err("a sealed Java class (`permits`)".to_owned());
        }
        let mut class = Class {
            kind,
            access,
            name,
            type_params,
            extends,
            implements,
            fields: Vec::new(),
            methods: Vec::new(),
            nested: Vec::new(),
            annotations,
        };
        self.expect("{")?;
        if kind == ClassKind::Enum {
            self.enum_constants(&mut class)?;
        }
        while !self.eat("}") {
            self.member(&mut class)?;
        }
        if kind == ClassKind::Class && !class.methods.iter().any(|m| m.name == nme::CONSTRUCTOR) {
            class.methods.push(constructor(class.access & VISIBILITY));
        }
        Ok(class)
    }

    fn types(&mut self) -> Result<Vec<Type>, String> {
        let mut types = vec![self.ty()?];
        while self.eat(",") {
            types.push(self.ty()?);
        }
        Ok(types)
    }

    /// Reads a member of `class`: an initializer, a nested type, a constructor, a method or the
    /// fields of one declaration, with the modifiers an interface implies.
    fn member(&mut self, class: &mut Class) -> Result<(), String> {
        if self.eat(";") {
            return Ok(());
        }
        let interface = class.access & ACC_INTERFACE != 0;
        if self.is("{") || self.is_ident("static") && matches!(self.peek_at(1), Token::Op("{")) {
            self.eat_ident("static");
            self.expect("{")?;
            return self.skip_balanced("{", "}");
        }
        let (mut access, annotations) = self.modifiers()?;
        let keyword = |w: &String| TYPE_KEYWORDS.contains(&w.as_str());
        if matches!(self.peek(), Token::Ident(w) if keyword(w)) || self.is("@") {
            let nested = self.type_decl(access, annotations, interface)?;
            class.nested.push(nested);
            return Ok(());
        }
        let type_params = self.type_params()?;
        let is_constructor = self.is_ident(&class.name) && matches!(self.peek_at(1), Token::Op("("));
        let (name, result) = if is_constructor {
            self.ident()?;
            (nme::CONSTRUCTOR.to_owned(), Type::Primitive('V'))
        } else {
            let result = self.ty()?;
            (self.ident()?, result)
        };
        if !self.is("(") {
            if interface {
                access |= ACC_PUBLIC | ACC_STATIC | ACC_FINAL;
            }
            return self.fields(class, access, result, name);
        }
        if interface && access & ACC_PRIVATE == 0 {
            access |= ACC_PUBLIC;
            if access & (DEFAULT | ACC_STATIC) == 0 {
                access |= ACC_ABSTRACT;
            }
        }
        let (params, varargs) = self.params()?;
        access |= if varargs { ACC_VARARGS } else { 0 };
        if self.is("[") {
            return Err(format!("array brackets after the parameters of method `{name}`"));
        }
        if self.eat_ident("throws") {
            self.types()?;
        }
        let annotation_default = self.eat_ident("default");
        if annotation_default {
            self.skip_to_comma_or_semi()?;
        }
        if self.eat("{") {
            self.skip_balanced("{", "}")?;
        } else {
            self.expect(";")?;
        }
        let access = access & !DEFAULT;
        class.methods.push(Method { access, name, type_params, params, result, annotation_default });
        Ok(())
    }

    /// Reads a parameter list: the parameter types, and whether the last is a varargs one.
    fn params(&mut self) -> Result<(Vec<Type>, bool), String> {
        self.expect("(")?;
        let mut params = Vec::new();
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
            params.push(t);
            if !self.eat(",") {
                self.expect(")")?;
                break;
            }
        }
        Ok((params, varargs))
    }
}

/// A constructor of access flags `access` without parameters.
fn constructor(access: u16) -> Method {
    let name = nme::CONSTRUCTOR.to_owned();
    let (type_params, params, result) = (Vec::new(), Vec::new(), Type::Primitive('V'));
    Method { access, name, type_params, params, result, annotation_default: false }
}
