//! The constant pool of a class file in ASM's order: the entries in the order `ClassNode.accept`
//! visits them, a method's code by `Code::remap`, then the attributes' names in the order of
//! `ClassWriter.toByteArray`, which is also the order the attributes are written in.

mod code;

use crate::attributes::Attribute;
use crate::pool::{Import, Pool};
use crate::{ClassFile, Lub, Member, writer};

/// The order of attributes, by name, in classes, fields and methods; the custom attributes
/// follow in the reverse order of their addition.
const ORDER: [&str; 12] = [
    "InnerClasses",
    "EnclosingMethod",
    "Code",
    "ConstantValue",
    "Exceptions",
    "Signature",
    "SourceFile",
    "Deprecated",
    "RuntimeVisibleAnnotations",
    "RuntimeInvisibleAnnotations",
    "MethodParameters",
    "BootstrapMethods",
];

/// The order in which ASM's visitors add the entries of attributes, by name, the class's
/// `Signature` with its name.
const VISITS: [&str; 9] = [
    "Signature",
    "SourceFile",
    "EnclosingMethod",
    "ConstantValue",
    "Exceptions",
    "MethodParameters",
    "RuntimeVisibleAnnotations",
    "RuntimeInvisibleAnnotations",
    "InnerClasses",
];

/// An attribute whose entries are in the pool: its name and index, its body, and for `Code`
/// the attributes of the code.
pub struct Laid {
    pub name: String,
    pub index: u16,
    pub body: Vec<u8>,
    pub nested: Option<Vec<Laid>>,
}

impl Laid {
    fn new(name: &str, body: Vec<u8>) -> Laid {
        Laid { name: name.to_owned(), index: 0, body, nested: None }
    }
}

/// A member whose entries are in the pool.
pub struct LaidMember {
    pub access: u16,
    pub name: u16,
    pub descriptor: u16,
    pub attributes: Vec<Laid>,
}

impl ClassFile {
    /// The class file's bytes, or why it cannot hold the class; `lub` gives the common superclass
    /// of two classes for the stack map frames.
    pub fn bytes(self, lub: &mut Lub) -> Result<Vec<u8>, String> {
        let mut im = Import::new(&self.pool);
        let this = im.to.class(&self.name);
        if let Some(Attribute::Signature(s)) =
            self.attributes.iter().find(|a| matches!(a, Attribute::Signature(_)))
        {
            im.to.utf8(s);
        }
        let superclass = im.to.class(&self.superclass);
        let interfaces: Vec<u16> = self.interfaces.iter().map(|i| im.to.class(i)).collect();
        let mut attributes = visit(&self.attributes, &mut im.to);

        let mut fields: Vec<LaidMember> = Vec::new();
        for f in &self.fields {
            fields.push(member(f, &mut im.to));
        }
        let mut methods: Vec<LaidMember> = Vec::new();
        for mut m in self.methods {
            let mut laid = member(&m, &mut im.to);
            if let Some(c) = m.code.take() {
                let c = code::code(c, &self.name, &m, &mut im, lub)
                    .map_err(|e| format!("method {}: {e}", m.name))?;
                insert(&mut laid.attributes, c);
            }
            methods.push(laid);
        }

        let Import { to: mut pool, .. } = im;
        if let Some(body) = pool.bootstrap_methods() {
            insert(&mut attributes, Laid::new("BootstrapMethods", body));
        }
        for m in fields.iter_mut().chain(&mut methods) {
            names(&mut m.attributes, &mut pool);
        }
        names(&mut attributes, &mut pool);
        let class =
            writer::Class { access: self.access, this, superclass, interfaces, fields, methods, attributes };
        writer::bytes(&pool, &class)
    }
}

/// Member `m` with the entries of its name, descriptor and attributes but its code in `pool`.
fn member(m: &Member, pool: &mut Pool) -> LaidMember {
    let (name, descriptor) = (pool.utf8(&m.name), pool.utf8(&m.descriptor));
    LaidMember { access: m.access, name, descriptor, attributes: visit(&m.attributes, pool) }
}

/// `attrs` with their entries added to `pool` in the order of `VISITS`, in the order of `ORDER`.
fn visit(attrs: &[Attribute], pool: &mut Pool) -> Vec<Laid> {
    let visit_rank = |a: &Attribute| VISITS.iter().position(|&v| v == a.name()).unwrap_or(VISITS.len());
    let mut by_visit: Vec<(usize, &Attribute)> = attrs.iter().enumerate().collect();
    by_visit.sort_by_key(|&(_, a)| visit_rank(a));
    let mut laid: Vec<(usize, Laid)> =
        by_visit.into_iter().map(|(i, a)| (i, Laid::new(a.name(), a.body(pool)))).collect();
    laid.sort_by_key(|(i, l)| match rank(l) {
        r if r == ORDER.len() => (r, usize::MAX - i),
        r => (r, *i),
    });
    laid.into_iter().map(|(_, l)| l).collect()
}

/// Inserts `a` into `attrs`, which are in the order of `ORDER`, at its place.
fn insert(attrs: &mut Vec<Laid>, a: Laid) {
    let at = attrs.iter().position(|b| rank(b) > rank(&a)).unwrap_or(attrs.len());
    attrs.insert(at, a);
}

fn rank(l: &Laid) -> usize {
    ORDER.iter().position(|&o| o == l.name).unwrap_or(ORDER.len())
}

/// Adds the names of `attrs` and of the attributes nested in them to `pool`, in their order.
fn names(attrs: &mut [Laid], pool: &mut Pool) {
    for a in attrs {
        a.index = pool.utf8(&a.name);
        if let Some(nested) = &mut a.nested {
            names(nested, pool);
        }
    }
}
