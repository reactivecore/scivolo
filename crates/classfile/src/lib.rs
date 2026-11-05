//! JVM class files (JVMS §4): a class with its constant pool, fields, methods and their code.

mod code;
mod pool;
mod writer;

pub use code::{Code, FieldOp, Invoke, Label, method_slots, split_method, value_slots};
pub use pool::{Pool, Ref};

/// Java 5's class file version, which needs no stack map frames.
pub const MAJOR_VERSION: u16 = 49;

pub const ACC_PUBLIC: u16 = 0x0001;
pub const ACC_PRIVATE: u16 = 0x0002;
pub const ACC_PROTECTED: u16 = 0x0004;
pub const ACC_STATIC: u16 = 0x0008;
pub const ACC_FINAL: u16 = 0x0010;
pub const ACC_SUPER: u16 = 0x0020;
pub const ACC_INTERFACE: u16 = 0x0200;
pub const ACC_ABSTRACT: u16 = 0x0400;
pub const ACC_SYNTHETIC: u16 = 0x1000;

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
    access: u16,
    this: u16,
    superclass: u16,
    interfaces: Vec<u16>,
    fields: Vec<Member>,
    methods: Vec<Member>,
    attributes: Vec<Attribute>,
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
            access,
            this,
            superclass,
            interfaces,
            fields: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            error: None,
        }
    }

    pub fn field(&mut self, access: u16, name: &str, descriptor: &str) {
        let member = self.member(access, name, descriptor);
        self.fields.push(member);
    }

    /// A method, abstract or native without `code`.
    pub fn method(&mut self, access: u16, name: &str, descriptor: &str, code: Option<Code>) {
        let mut member = self.member(access, name, descriptor);
        if let Some(code) = code {
            if code.bytes.len() > usize::from(u16::MAX) {
                self.error = Some(format!("method {name} over 65,535 bytes of code"));
            }
            if let Some(e) = code.problem() {
                self.error = Some(format!("method {name}: {e}"));
            }
            let name = self.pool.utf8("Code");
            member.attributes.push(Attribute { name, bytes: writer::code(&code) });
        }
        self.methods.push(member);
    }

    /// The `SourceFile` attribute.
    pub fn source_file(&mut self, file: &str) {
        self.utf8_attribute("SourceFile", file);
    }

    /// The class's generic `Signature` attribute.
    pub fn signature(&mut self, signature: &str) {
        self.utf8_attribute("Signature", signature);
    }

    /// Attribute `name` holding the index of `value`.
    fn utf8_attribute(&mut self, name: &str, value: &str) {
        let name = self.pool.utf8(name);
        let bytes = self.pool.utf8(value).to_be_bytes().to_vec();
        self.attributes.push(Attribute { name, bytes });
    }

    fn member(&mut self, access: u16, name: &str, descriptor: &str) -> Member {
        let (name, descriptor) = (self.pool.utf8(name), self.pool.utf8(descriptor));
        Member { access, name, descriptor, attributes: Vec::new() }
    }
}
