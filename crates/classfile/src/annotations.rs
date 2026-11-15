//! Annotations written into class files (JVMS §4.7.16), their constant-pool entries added in
//! ASM's `AnnotationWriter` order.

use crate::Pool;

/// An annotation: its type's descriptor and its elements with their values, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct AnnotationValue {
    pub descriptor: String,
    pub elements: Vec<(String, ElementValue)>,
}

/// The value of an annotation element (JVMS §4.7.16.1).
#[derive(Clone, Debug, PartialEq)]
pub enum ElementValue {
    Boolean(bool),
    Byte(i8),
    Char(u16),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
    /// An enum constant: its class's descriptor and its name.
    Enum(String, String),
    /// A class literal by its return descriptor, e.g. `Ljava/lang/String;` or `V`.
    Class(String),
    Annotation(AnnotationValue),
    Array(Vec<ElementValue>),
}

/// Encodes the bytes of an annotations attribute of `annotations`, adding to `descriptors` the
/// `Utf8` entries of the descriptors they name.
pub fn encode(pool: &mut Pool, annotations: &[AnnotationValue], descriptors: &mut Vec<u16>) -> Vec<u8> {
    let mut e = Encoder { pool, out: (annotations.len() as u16).to_be_bytes().to_vec(), descriptors };
    annotations.iter().for_each(|a| e.annotation(a));
    e.out
}

/// The bytes of an annotations attribute being encoded and the descriptors they name.
struct Encoder<'a> {
    pool: &'a mut Pool,
    out: Vec<u8>,
    descriptors: &'a mut Vec<u16>,
}

impl Encoder<'_> {
    fn u2(&mut self, v: u16) {
        self.out.extend(v.to_be_bytes());
    }

    fn descriptor(&mut self, d: &str) -> u16 {
        let i = self.pool.utf8(d);
        self.descriptors.push(i);
        i
    }

    fn annotation(&mut self, a: &AnnotationValue) {
        let descriptor = self.descriptor(&a.descriptor);
        self.u2(descriptor);
        self.u2(a.elements.len() as u16);
        for (name, value) in &a.elements {
            let name = self.pool.utf8(name);
            self.u2(name);
            self.element(value);
        }
    }

    fn element(&mut self, value: &ElementValue) {
        let pool = &mut *self.pool;
        let (tag, index) = match value {
            ElementValue::Boolean(b) => (b'Z', pool.integer(i32::from(*b))),
            ElementValue::Byte(b) => (b'B', pool.integer(i32::from(*b))),
            ElementValue::Char(c) => (b'C', pool.integer(i32::from(*c))),
            ElementValue::Short(s) => (b'S', pool.integer(i32::from(*s))),
            ElementValue::Int(i) => (b'I', pool.integer(*i)),
            ElementValue::Long(l) => (b'J', pool.long(*l)),
            ElementValue::Float(f) => (b'F', pool.float(*f)),
            ElementValue::Double(d) => (b'D', pool.double(*d)),
            ElementValue::String(s) => (b's', pool.utf8(s)),
            ElementValue::Class(d) => (b'c', self.descriptor(d)),
            ElementValue::Enum(descriptor, name) => {
                self.out.push(b'e');
                let descriptor = self.descriptor(descriptor);
                self.u2(descriptor);
                let name = self.pool.utf8(name);
                self.u2(name);
                return;
            }
            ElementValue::Annotation(a) => {
                self.out.push(b'@');
                self.annotation(a);
                return;
            }
            ElementValue::Array(values) => {
                self.out.push(b'[');
                self.u2(values.len() as u16);
                values.iter().for_each(|v| self.element(v));
                return;
            }
        };
        self.out.push(tag);
        self.u2(index);
    }
}
