//! JVM class files (JVMS §4): a class with its constant pool, fields, methods and their code.

mod annotations;
mod attributes;
mod code;
mod frames;
mod kind;
mod layout;
mod modified_utf8;
mod opcodes;
mod pool;
mod read;
mod references;
mod writer;

pub use annotations::{AnnotationValue, ElementValue};
use attributes::Attribute;
pub use attributes::{InnerClass, Last};
pub use code::{Code, FieldOp, Invoke, Label, method_slots, split_method, value_slots};
pub use frames::Lub;
pub use kind::Kind;
pub use opcodes::*;
pub use pool::{
    Constant, Handle, Pool, REF_INVOKE_INTERFACE, REF_INVOKE_SPECIAL, REF_INVOKE_STATIC, REF_INVOKE_VIRTUAL,
    Ref,
};
pub use read::{Annotation, ClassInfo, ConstantValue, Jdk, MemberInfo, read};

/// Java 17's class file version, dotty's default target.
pub const MAJOR_VERSION: u16 = 61;

pub const ACC_PUBLIC: u16 = 0x0001;
pub const ACC_PRIVATE: u16 = 0x0002;
pub const ACC_PROTECTED: u16 = 0x0004;
pub const ACC_STATIC: u16 = 0x0008;
pub const ACC_FINAL: u16 = 0x0010;
pub const ACC_SUPER: u16 = 0x0020;
pub const ACC_INTERFACE: u16 = 0x0200;
pub const ACC_ABSTRACT: u16 = 0x0400;
pub const ACC_BRIDGE: u16 = 0x0040;
pub const ACC_VOLATILE: u16 = 0x0040;
pub const ACC_TRANSIENT: u16 = 0x0080;
pub const ACC_VARARGS: u16 = 0x0080;
pub const ACC_NATIVE: u16 = 0x0100;
pub const ACC_SYNTHETIC: u16 = 0x1000;
pub const ACC_ANNOTATION: u16 = 0x2000;
pub const ACC_ENUM: u16 = 0x4000;
pub const ACC_MODULE: u16 = 0x8000;

/// A field or method: its flags, name, descriptor, code and other attributes.
struct Member {
    access: u16,
    name: String,
    descriptor: String,
    code: Option<Code>,
    attributes: Vec<Attribute>,
}

/// A class file being built; its constant pool is laid out in ASM's order when it is written.
pub struct ClassFile {
    /// The pool the code of the methods refers to.
    pub pool: Pool,
    /// The class's internal name.
    name: String,
    access: u16,
    superclass: String,
    interfaces: Vec<String>,
    fields: Vec<Member>,
    methods: Vec<Member>,
    /// The class's attributes, those the JVM does not define in the order they were added.
    attributes: Vec<Attribute>,
}

impl ClassFile {
    /// Class `name` extending `superclass`, all three internal names.
    pub fn new(access: u16, name: &str, superclass: &str, interfaces: &[&str]) -> ClassFile {
        ClassFile {
            pool: Pool::default(),
            name: name.to_owned(),
            access,
            superclass: superclass.to_owned(),
            interfaces: interfaces.iter().map(|&i| i.to_owned()).collect(),
            fields: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
        }
    }

    pub fn field(&mut self, access: u16, name: &str, descriptor: &str) {
        self.fields.push(member(access, name, descriptor, None));
    }

    /// A method, abstract or native without `code`, which refers to `pool`.
    pub fn method(&mut self, access: u16, name: &str, descriptor: &str, code: Option<Code>) {
        self.methods.push(member(access, name, descriptor, code));
    }

    /// The number of methods added.
    pub fn method_count(&self) -> usize {
        self.methods.len()
    }

    /// Moves the methods added from the `from`th on to the `to`th place, before those after it.
    pub fn move_methods(&mut self, from: usize, to: usize) {
        let len = self.methods.len();
        if to <= from && from <= len {
            self.methods[to..].rotate_right(len - from);
        }
    }
}

fn member(access: u16, name: &str, descriptor: &str, code: Option<Code>) -> Member {
    let (name, descriptor) = (name.to_owned(), descriptor.to_owned());
    Member { access, name, descriptor, code, attributes: Vec::new() }
}
