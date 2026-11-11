//! Attributes of classes and members besides `Code` (JVMS §4.7).

use crate::{Attribute, ClassFile};

/// A member of the `InnerClasses` attribute: the class, its outer class and simple name (none
/// for a local or anonymous class), and its flags.
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

impl ClassFile {
    /// The `SourceFile` attribute.
    pub fn source_file(&mut self, file: &str) {
        let bytes = self.pool.utf8(file).to_be_bytes().to_vec();
        self.attribute("SourceFile", bytes);
    }

    /// The class's generic `Signature` attribute.
    pub fn signature(&mut self, signature: &str) {
        let bytes = self.pool.utf8(signature).to_be_bytes().to_vec();
        self.attribute("Signature", bytes);
    }

    /// The `Deprecated` attribute of the class.
    pub fn deprecated(&mut self) {
        self.attribute("Deprecated", Vec::new());
    }

    /// The `InnerClasses` attribute.
    pub fn inner_classes(&mut self, classes: &[InnerClass]) {
        let mut bytes = (classes.len() as u16).to_be_bytes().to_vec();
        for c in classes {
            let inner = self.pool.class(&c.inner);
            let outer = c.outer.as_ref().map_or(0, |o| self.pool.class(o));
            let name = c.name.as_ref().map_or(0, |n| self.pool.utf8(n));
            for v in [inner, outer, name, c.flags] {
                bytes.extend(v.to_be_bytes());
            }
        }
        self.attribute("InnerClasses", bytes);
    }

    /// The `EnclosingMethod` attribute: the enclosing class and method, by name and descriptor.
    pub fn enclosing_method(&mut self, class: &str, method: Option<(&str, &str)>) {
        let mut bytes = self.pool.class(class).to_be_bytes().to_vec();
        let method = method.map_or(0, |(n, d)| self.pool.name_and_type(n, d));
        bytes.extend(method.to_be_bytes());
        self.attribute("EnclosingMethod", bytes);
    }

    /// The generic `Signature` attribute of the last member.
    pub fn member_signature(&mut self, last: Last, signature: &str) {
        let bytes = self.pool.utf8(signature).to_be_bytes().to_vec();
        self.member_attribute(last, "Signature", bytes);
    }

    /// The `ConstantValue` attribute of the last field, a `long`.
    pub fn long_constant_value(&mut self, value: i64) {
        let bytes = self.pool.long(value).to_be_bytes().to_vec();
        self.member_attribute(Last::Field, "ConstantValue", bytes);
    }

    /// The `Deprecated` attribute of the last member.
    pub fn member_deprecated(&mut self, last: Last) {
        self.member_attribute(last, "Deprecated", Vec::new());
    }

    /// The `MethodParameters` attribute of the last method: each parameter's name and flags.
    pub fn method_parameters(&mut self, params: &[(String, u16)]) {
        let mut bytes = vec![params.len() as u8];
        for (name, flags) in params {
            bytes.extend(self.pool.utf8(name).to_be_bytes());
            bytes.extend(flags.to_be_bytes());
        }
        self.member_attribute(Last::Method, "MethodParameters", bytes);
    }

    /// The `Exceptions` attribute of the last method: the internal names of the classes it throws.
    pub fn exceptions(&mut self, classes: &[String]) {
        let mut bytes = (classes.len() as u16).to_be_bytes().to_vec();
        for c in classes {
            bytes.extend(self.pool.class(c).to_be_bytes());
        }
        self.member_attribute(Last::Method, "Exceptions", bytes);
    }

    /// The `RuntimeVisibleAnnotations` attribute, else `RuntimeInvisibleAnnotations`, of the class
    /// or of the last member: annotations without elements, by their types' descriptors.
    pub fn annotations(&mut self, last: Option<Last>, visible: bool, descriptors: &[String]) {
        let mut bytes = (descriptors.len() as u16).to_be_bytes().to_vec();
        for d in descriptors {
            bytes.extend(self.pool.utf8(d).to_be_bytes());
            bytes.extend(0u16.to_be_bytes());
        }
        let name = if visible { "RuntimeVisibleAnnotations" } else { "RuntimeInvisibleAnnotations" };
        match last {
            Some(last) => self.member_attribute(last, name, bytes),
            None => self.attribute(name, bytes),
        }
    }

    fn member_attribute(&mut self, last: Last, name: &str, bytes: Vec<u8>) {
        let name = self.pool.utf8(name);
        let members = match last {
            Last::Field => &mut self.fields,
            Last::Method => &mut self.methods,
        };
        let member = members.last_mut().expect("a member to add an attribute to");
        member.attributes.push(Attribute { name, bytes });
    }

    /// An attribute the JVM does not define, written after the others.
    pub fn custom_attribute(&mut self, name: &str, bytes: Vec<u8>) {
        let name = self.pool.utf8(name);
        self.custom.push(Attribute { name, bytes });
    }

    fn attribute(&mut self, name: &str, bytes: Vec<u8>) {
        let name = self.pool.utf8(name);
        self.attributes.push(Attribute { name, bytes });
    }
}
