//! The declarations of a Java source as its parser reads them: names not yet resolved, bodies and
//! initializers left out.

use classfile::ConstantValue;

/// A Java compilation unit: its package (dotted), imports and top-level classes.
pub struct Unit {
    pub package: String,
    pub imports: Vec<Import>,
    pub classes: Vec<Class>,
}

/// An import: `a.b.C` (single) or `a.b.*` (on demand); static imports are left out.
pub struct Import {
    pub path: Vec<String>,
    pub on_demand: bool,
}

/// What a Java type declaration declares.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    Interface,
    Enum,
    Annotation,
    Record,
}

/// A Java class, interface, enum, record or annotation type: its class-file access flags, type
/// parameters, parents, whether it is `sealed` and its `permits` clause, members and nested
/// classes.
pub struct Class {
    pub kind: ClassKind,
    pub access: u16,
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub extends: Option<Type>,
    pub implements: Vec<Type>,
    pub sealed: bool,
    pub permitted: Vec<Type>,
    pub fields: Vec<Field>,
    pub methods: Vec<Method>,
    pub nested: Vec<Class>,
    pub annotations: Vec<Annotation>,
}

/// A type parameter and its bounds, `Object` without any.
pub struct TypeParam {
    pub name: String,
    pub bounds: Vec<Type>,
}

/// A Java type as written.
#[derive(Clone)]
pub enum Type {
    /// A primitive type or `void`, by its descriptor letter.
    Primitive(char),
    /// A possibly qualified name, each part with its type arguments.
    Named(Vec<(String, Vec<Arg>)>),
    Array(Box<Type>),
}

/// A type argument.
#[derive(Clone)]
pub enum Arg {
    Type(Type),
    Wildcard,
    Extends(Type),
    Super(Type),
}

/// A field with its class-file access flags, constant value and annotations, none if the parser
/// does not keep them.
pub struct Field {
    pub access: u16,
    pub name: String,
    pub ty: Type,
    pub constant: Option<ConstantValue>,
    pub annotations: Option<Vec<Annotation>>,
}

/// A method or constructor (named `<init>`) with its class-file access flags and its
/// annotations, none if the parser does not keep them.
pub struct Method {
    pub access: u16,
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Type>,
    pub result: Type,
    pub annotation_default: bool,
    pub annotations: Option<Vec<Annotation>>,
}

/// An annotation: its type's name, the enum constants of its arguments by element, the class
/// names of its class literal arguments by element, and its arguments by element, none if
/// dotty's parser drops the annotation.
#[derive(Clone)]
pub struct Annotation {
    pub name: Vec<String>,
    pub enum_elements: Vec<(String, String)>,
    pub class_elements: Vec<(String, Vec<String>)>,
    pub args: Option<Vec<(String, ArgValue)>>,
}

/// The value of an annotation element as written.
#[derive(Clone)]
pub enum ArgValue {
    /// A literal: a boolean, character, number or string.
    Literal(classfile::ElementValue),
    Array(Vec<ArgValue>),
    /// A possibly qualified name: an enum constant or a constant field.
    Path(Vec<String>),
    Annotation(Annotation),
    /// A class literal `p.C.class`: the class's name.
    ClassLiteral(Vec<String>),
}
