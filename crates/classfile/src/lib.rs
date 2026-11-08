//! JVM class files (JVMS §4): a class with its constant pool, fields, methods and their code.

mod attributes;
mod code;
mod frames;
mod modified_utf8;
mod pool;
mod read;
mod references;
mod writer;

pub use attributes::{InnerClass, Last};
pub use code::{Code, FieldOp, Invoke, Label, method_slots, split_method, value_slots};
pub use frames::Lub;
pub use pool::{
    Constant, Handle, Pool, REF_INVOKE_INTERFACE, REF_INVOKE_SPECIAL, REF_INVOKE_STATIC, REF_INVOKE_VIRTUAL,
    Ref,
};
pub use read::{ClassInfo, ConstantValue, Jdk, MemberInfo, read};

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
pub const ACC_VARARGS: u16 = 0x0080;
pub const ACC_NATIVE: u16 = 0x0100;
pub const ACC_SYNTHETIC: u16 = 0x1000;
pub const ACC_ANNOTATION: u16 = 0x2000;
pub const ACC_ENUM: u16 = 0x4000;
pub const ACC_MODULE: u16 = 0x8000;

/// An attribute: its name and its bytes.
struct Attribute {
    name: u16,
    bytes: Vec<u8>,
}

struct Member {
    access: u16,
    name: u16,
    descriptor: u16,
    attributes: Vec<Attribute>,
}

pub struct ClassFile {
    pub pool: Pool,
    /// The class's internal name.
    name: String,
    access: u16,
    this: u16,
    superclass: u16,
    interfaces: Vec<u16>,
    fields: Vec<Member>,
    methods: Vec<Member>,
    attributes: Vec<Attribute>,
    /// The attributes the JVM does not define, written last.
    custom: Vec<Attribute>,
    /// Why the class file cannot hold a method added.
    error: Option<String>,
}

impl ClassFile {
    /// Class `name` extending `superclass`, all three internal names.
    pub fn new(access: u16, name: &str, superclass: &str, interfaces: &[&str]) -> ClassFile {
        let mut pool = Pool::default();
        let this = pool.class(name);
        let superclass = pool.class(superclass);
        let interfaces = interfaces.iter().map(|i| pool.class(i)).collect();
        ClassFile {
            pool,
            name: name.to_owned(),
            access,
            this,
            superclass,
            interfaces,
            fields: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            custom: Vec::new(),
            error: None,
        }
    }

    /// Whether the class declares method `name` of `descriptor` with all of flags `access`.
    pub fn declares(&self, access: u16, name: &str, descriptor: &str) -> bool {
        let (Some(n), Some(d)) = (self.pool.find_utf8(name), self.pool.find_utf8(descriptor)) else {
            return false;
        };
        self.methods.iter().any(|m| m.name == n && m.descriptor == d && m.access & access == access)
    }

    pub fn field(&mut self, access: u16, name: &str, descriptor: &str) {
        let member = self.member(access, name, descriptor);
        self.fields.push(member);
    }

    /// A method, abstract or native without `code`; `lub` gives the common superclass of two
    /// classes for its stack map frames.
    pub fn method(&mut self, access: u16, name: &str, descriptor: &str, code: Option<Code>, lub: &mut Lub) {
        let mut member = self.member(access, name, descriptor);
        if let Some(mut code) = code {
            let frames = self.frames(&mut code, access, name, descriptor, lub).unwrap_or_else(|e| {
                self.error = Some(format!("method {name}: {e}"));
                None
            });
            let mut attrs = Vec::new();
            if let Some(bytes) = code.line_table() {
                attrs.push(Attribute { name: self.pool.utf8("LineNumberTable"), bytes });
            }
            if let Some(bytes) = frames {
                attrs.push(Attribute { name: self.pool.utf8("StackMapTable"), bytes });
            }
            let bytes = writer::code(&code, &attrs).unwrap_or_else(|e| {
                self.error = Some(format!("method {name}: {e}"));
                Vec::new()
            });
            member.attributes.push(Attribute { name: self.pool.utf8("Code"), bytes });
        }
        self.methods.push(member);
    }

    /// Checks `code`; the body of its `StackMapTable`, none if it needs none.
    fn frames(
        &mut self,
        code: &mut Code,
        access: u16,
        name: &str,
        descriptor: &str,
        lub: &mut Lub,
    ) -> Result<Option<Vec<u8>>, String> {
        if code.bytes.len() > usize::from(u16::MAX) {
            return Err("over 65,535 bytes of code".into());
        }
        if let Some(e) = code.problem() {
            return Err(e);
        }
        let m = frames::Method { class: &self.name, is_static: access & ACC_STATIC != 0, name, descriptor };
        frames::stack_map(code, &mut self.pool, &m, lub)
    }

    fn member(&mut self, access: u16, name: &str, descriptor: &str) -> Member {
        let (name, descriptor) = (self.pool.utf8(name), self.pool.utf8(descriptor));
        Member { access, name, descriptor, attributes: Vec::new() }
    }
}
