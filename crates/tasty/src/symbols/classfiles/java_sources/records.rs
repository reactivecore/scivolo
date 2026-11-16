// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/parsing/JavaParsers.scala
//! The members javac adds to a Java record (dotty's `recordDecl`): the canonical constructor,
//! the component fields and their accessors.

use super::parser::Parser;
use super::scanner::Token;
use super::syntax::{Class, ClassKind, Field, Method, Type};
use classfile::{ACC_FINAL, ACC_PRIVATE, ACC_PUBLIC, ACC_VARARGS};
use names::nme;

/// The access flags that say who sees a declaration.
pub const VISIBILITY: u16 = classfile::ACC_PUBLIC | classfile::ACC_PROTECTED | classfile::ACC_PRIVATE;

/// The components of a record header `(T a, U... b)`, and whether the last is a varargs one.
pub struct Header {
    pub components: Vec<(String, Type)>,
    pub varargs: bool,
}

/// A constructor of access flags `access` with parameters `params`.
pub fn constructor(access: u16, params: Vec<Type>) -> Method {
    let (name, type_params, result) = (nme::CONSTRUCTOR.to_owned(), Vec::new(), Type::Primitive('V'));
    Method { access, name, type_params, params, result, annotation_default: false, annotations: None }
}

impl Parser {
    /// Skips a compact constructor `Name { … }` of record `class` if the cursor is at one.
    pub fn compact_constructor(&mut self, class: &Class) -> Result<bool, String> {
        let compact = class.kind == ClassKind::Record
            && self.is_ident(&class.name)
            && matches!(self.peek_at(1), Token::Op("{"));
        if compact {
            self.at += 2;
            self.skip_balanced("{", "}")?;
        }
        Ok(compact)
    }
}

/// Adds the members javac adds to record `class` of header `header`: a private final field and
/// a public accessor per component (the accessor of a varargs one returns its array) unless the
/// body declares one, and the canonical constructor of the record's access unless declared.
pub fn complete(class: &mut Class, header: Header) {
    let types: Vec<Type> = header.components.iter().map(|(_, t)| t.clone()).collect();
    for (name, ty) in header.components {
        let declared = class.methods.iter().any(|m| m.name == name && m.params.is_empty());
        if !declared {
            let (type_params, result) = (Vec::new(), ty.clone());
            let (access, name) = (ACC_PUBLIC, name.clone());
            class.methods.push(Method {
                access,
                name,
                type_params,
                params: Vec::new(),
                result,
                annotation_default: false,
                annotations: None,
            });
        }
        class.fields.push(Field {
            access: ACC_PRIVATE | ACC_FINAL,
            name,
            ty,
            constant: None,
            annotations: None,
        });
    }
    let canonical = class.methods.iter().any(|m| m.name == nme::CONSTRUCTOR && same_types(&m.params, &types));
    if !canonical {
        let varargs = if header.varargs { ACC_VARARGS } else { 0 };
        class.methods.push(constructor(class.access & VISIBILITY | varargs, types));
    }
}

/// Determines whether types `a` and `b` are written the same.
fn same_types(a: &[Type], b: &[Type]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| written(x) == written(y))
}

/// Type `t` as text, for comparing types as written.
fn written(t: &Type) -> String {
    match t {
        Type::Primitive(c) => c.to_string(),
        Type::Array(e) => format!("[{}", written(e)),
        Type::Named(parts) => {
            parts.iter().map(|(n, args)| format!("{n}<{}>", args.len())).collect::<Vec<_>>().join(".")
        }
    }
}
