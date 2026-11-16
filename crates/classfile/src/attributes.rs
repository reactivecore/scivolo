//! Attributes of classes and members besides `Code` (JVMS §4.7).

use crate::{AnnotationValue, ClassFile, Pool};

/// A member of the `InnerClasses` attribute: the class, its outer class and simple name (none
/// for a local or anonymous class), and its flags.
#[derive(Clone, Debug, PartialEq)]
pub struct InnerClass {
    pub inner: String,
    pub outer: Option<String>,
    pub name: Option<String>,
    pub flags: u16,
}

/// The member an attribute goes to.
#[derive(Clone, Copy)]
pub enum Last {
    Field,
    Method,
}

/// An attribute by the names and values it refers to.
pub enum Attribute {
    InnerClasses(Vec<InnerClass>),
    /// The enclosing class and method, by name and descriptor.
    EnclosingMethod(String, Option<(String, String)>),
    /// A `long` field's value.
    ConstantValue(i64),
    /// The classes a method throws.
    Exceptions(Vec<String>),
    Signature(String),
    SourceFile(String),
    Deprecated,
    /// Annotations, visible at run time or not.
    Annotations(bool, Vec<AnnotationValue>),
    /// Each parameter's annotations, visible at run time or not.
    ParameterAnnotations(bool, Vec<Vec<AnnotationValue>>),
    /// Each parameter's name and flags.
    MethodParameters(Vec<(String, u16)>),
    /// An attribute the JVM does not define.
    Custom(String, Vec<u8>),
}

impl Attribute {
    pub fn name(&self) -> &str {
        match self {
            Attribute::InnerClasses(_) => "InnerClasses",
            Attribute::EnclosingMethod(..) => "EnclosingMethod",
            Attribute::ConstantValue(_) => "ConstantValue",
            Attribute::Exceptions(_) => "Exceptions",
            Attribute::Signature(_) => "Signature",
            Attribute::SourceFile(_) => "SourceFile",
            Attribute::Deprecated => "Deprecated",
            Attribute::Annotations(true, _) => "RuntimeVisibleAnnotations",
            Attribute::Annotations(false, _) => "RuntimeInvisibleAnnotations",
            Attribute::ParameterAnnotations(true, _) => "RuntimeVisibleParameterAnnotations",
            Attribute::ParameterAnnotations(false, _) => "RuntimeInvisibleParameterAnnotations",
            Attribute::MethodParameters(_) => "MethodParameters",
            Attribute::Custom(name, _) => name,
        }
    }

    /// Adds the entries the attribute refers to to `pool`; its body.
    pub fn body(&self, pool: &mut Pool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut put = |v: u16| out.extend(v.to_be_bytes());
        match self {
            Attribute::InnerClasses(classes) => {
                put(classes.len() as u16);
                for c in classes {
                    put(pool.class(&c.inner));
                    put(c.outer.as_ref().map_or(0, |o| pool.class(o)));
                    put(c.name.as_ref().map_or(0, |n| pool.utf8(n)));
                    put(c.flags);
                }
            }
            Attribute::EnclosingMethod(class, method) => {
                put(pool.class(class));
                put(method.as_ref().map_or(0, |(n, d)| pool.name_and_type(n, d)));
            }
            Attribute::ConstantValue(v) => put(pool.long(*v)),
            Attribute::Exceptions(classes) => {
                put(classes.len() as u16);
                classes.iter().for_each(|c| put(pool.class(c)));
            }
            Attribute::Signature(s) | Attribute::SourceFile(s) => put(pool.utf8(s)),
            Attribute::Deprecated => {}
            Attribute::Annotations(_, annotations) => {
                out.extend(crate::annotations::encode(pool, annotations))
            }
            Attribute::ParameterAnnotations(_, params) => {
                out.push(params.len() as u8);
                params
                    .iter()
                    .for_each(|annotations| out.extend(crate::annotations::encode(pool, annotations)));
            }
            Attribute::MethodParameters(params) => {
                out.push(params.len() as u8);
                for (name, flags) in params {
                    out.extend(pool.utf8(name).to_be_bytes());
                    out.extend(flags.to_be_bytes());
                }
            }
            Attribute::Custom(_, bytes) => out.extend(bytes),
        }
        out
    }
}

impl ClassFile {
    /// The `SourceFile` attribute.
    pub fn source_file(&mut self, file: &str) {
        self.attributes.push(Attribute::SourceFile(file.to_owned()));
    }

    /// The class's generic `Signature` attribute.
    pub fn signature(&mut self, signature: &str) {
        self.attributes.push(Attribute::Signature(signature.to_owned()));
    }

    /// The `Deprecated` attribute of the class.
    pub fn deprecated(&mut self) {
        self.attributes.push(Attribute::Deprecated);
    }

    /// The `InnerClasses` attribute.
    pub fn inner_classes(&mut self, classes: &[InnerClass]) {
        self.attributes.push(Attribute::InnerClasses(classes.to_vec()));
    }

    /// The `EnclosingMethod` attribute: the enclosing class and method, by name and descriptor.
    pub fn enclosing_method(&mut self, class: &str, method: Option<(&str, &str)>) {
        let method = method.map(|(n, d)| (n.to_owned(), d.to_owned()));
        self.attributes.push(Attribute::EnclosingMethod(class.to_owned(), method));
    }

    /// The generic `Signature` attribute of the last member.
    pub fn member_signature(&mut self, last: Last, signature: &str) {
        self.member_attribute(last, Attribute::Signature(signature.to_owned()));
    }

    /// The `ConstantValue` attribute of the last field, a `long`.
    pub fn long_constant_value(&mut self, value: i64) {
        self.member_attribute(Last::Field, Attribute::ConstantValue(value));
    }

    /// The `Deprecated` attribute of the last member.
    pub fn member_deprecated(&mut self, last: Last) {
        self.member_attribute(last, Attribute::Deprecated);
    }

    /// The `MethodParameters` attribute of the last method: each parameter's name and flags.
    pub fn method_parameters(&mut self, params: &[(String, u16)]) {
        self.member_attribute(Last::Method, Attribute::MethodParameters(params.to_vec()));
    }

    /// The `Exceptions` attribute of the last method: the internal names of the classes it throws.
    pub fn exceptions(&mut self, classes: &[String]) {
        self.member_attribute(Last::Method, Attribute::Exceptions(classes.to_vec()));
    }

    /// The `RuntimeVisibleAnnotations` attribute, else `RuntimeInvisibleAnnotations`, of the class
    /// or of the last member.
    pub fn annotations(&mut self, last: Option<Last>, visible: bool, annotations: &[AnnotationValue]) {
        let a = Attribute::Annotations(visible, annotations.to_vec());
        match last {
            Some(last) => self.member_attribute(last, a),
            None => self.attributes.push(a),
        }
    }

    /// The `RuntimeVisibleParameterAnnotations` attribute, else
    /// `RuntimeInvisibleParameterAnnotations`, of the last method: each parameter's annotations.
    pub fn parameter_annotations(&mut self, visible: bool, params: &[Vec<AnnotationValue>]) {
        self.member_attribute(Last::Method, Attribute::ParameterAnnotations(visible, params.to_vec()));
    }

    fn member_attribute(&mut self, last: Last, a: Attribute) {
        let members = match last {
            Last::Field => &mut self.fields,
            Last::Method => &mut self.methods,
        };
        members.last_mut().expect("a member to add an attribute to").attributes.push(a);
    }

    /// An attribute the JVM does not define; such attributes are written in the reverse order
    /// of their addition, after the others (ASM's `visitAttribute`).
    pub fn custom_attribute(&mut self, name: &str, bytes: Vec<u8>) {
        self.attributes.push(Attribute::Custom(name.to_owned(), bytes));
    }
}
