// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatureVisitor.scala
//! The classes a class file refers to whose names contain `$`, the candidates for its
//! `InnerClasses` attribute (dotty's `NestedClassesCollector`).

use crate::ClassFile;
use crate::attributes::Attribute;
use crate::pool::Entry;
use std::borrow::Cow;

impl ClassFile {
    /// The internal names with a `$` of the classes the class, its members, their descriptors and
    /// signatures, and its code refer to, each once; not those only in the descriptor of its
    /// enclosing method.
    pub fn referenced_classes(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut add = |name: &str| {
            if name.contains('$') && !out.iter().any(|o| o == name) {
                out.push(name.to_owned());
            }
        };
        let mut classes: Vec<Cow<str>> =
            [&self.name, &self.superclass].into_iter().chain(&self.interfaces).map(Cow::from).collect();
        let mut texts: Vec<Cow<str>> = Vec::new();
        let attributes =
            self.fields.iter().chain(&self.methods).flat_map(|m| &m.attributes).chain(&self.attributes);
        for a in attributes {
            let mut descriptors = Vec::new();
            match a {
                Attribute::Signature(s) => texts.push(Cow::from(s)),
                Attribute::Exceptions(cs) => classes.extend(cs.iter().map(Cow::from)),
                Attribute::EnclosingMethod(c, _) => classes.push(Cow::from(c)),
                Attribute::InnerClasses(cs) => classes
                    .extend(cs.iter().flat_map(|c| std::iter::once(&c.inner).chain(&c.outer)).map(Cow::from)),
                Attribute::Annotations(_, annotations) => {
                    annotations.iter().for_each(|a| a.descriptors(&mut descriptors))
                }
                Attribute::ParameterAnnotations(_, params) => {
                    params.iter().flatten().for_each(|a| a.descriptors(&mut descriptors))
                }
                _ => {}
            }
            texts.extend(descriptors.into_iter().map(Cow::Owned));
        }
        texts.extend(self.fields.iter().chain(&self.methods).map(|m| Cow::from(&m.descriptor)));
        for (i, e) in (1u16..).zip(self.pool.entries()) {
            match *e {
                Entry::Class(n) => classes.push(Cow::Owned(self.pool.text(n))),
                Entry::MethodType(d) => texts.push(Cow::Owned(self.pool.text(d))),
                Entry::Fieldref(..)
                | Entry::Methodref(..)
                | Entry::InterfaceMethodref(..)
                | Entry::InvokeDynamic(..) => texts.push(Cow::Owned(self.pool.member_parts(i).2)),
                _ => {}
            }
        }
        for c in classes {
            match c.starts_with('[') {
                true => texts.push(c),
                false => add(&c),
            }
        }
        for t in texts {
            Signature { s: t.as_bytes(), i: 0 }.all(&mut add);
        }
        out
    }
}

/// A reader of descriptors and generic signatures (JVMS §4.3, §4.7.9.1).
struct Signature<'s> {
    s: &'s [u8],
    i: usize,
}

impl Signature<'_> {
    fn peek(&self) -> u8 {
        self.s.get(self.i).copied().unwrap_or(b';')
    }

    fn all(&mut self, add: &mut impl FnMut(&str)) {
        if self.peek() == b'<' {
            self.i += 1;
            while self.i < self.s.len() && self.peek() != b'>' {
                while self.i < self.s.len() && self.peek() != b':' {
                    self.i += 1;
                }
                while self.peek() == b':' {
                    self.i += 1;
                    if matches!(self.peek(), b'L' | b'T' | b'[') {
                        self.ty(add);
                    }
                }
            }
            self.i += 1;
        }
        while self.i < self.s.len() {
            match self.peek() {
                b'(' | b')' | b'^' => self.i += 1,
                _ => self.ty(add),
            }
        }
    }

    fn ty(&mut self, add: &mut impl FnMut(&str)) {
        match self.peek() {
            b'L' => {
                self.i += 1;
                let mut name = String::new();
                loop {
                    let start = self.i;
                    while !matches!(self.peek(), b';' | b'<' | b'.') {
                        self.i += 1;
                    }
                    name.push_str(&String::from_utf8_lossy(&self.s[start..self.i]));
                    if self.peek() == b'<' {
                        self.i += 1;
                        while self.i < self.s.len() && self.peek() != b'>' {
                            match self.peek() {
                                b'*' | b'+' | b'-' => self.i += 1,
                                _ => self.ty(add),
                            }
                        }
                        self.i += 1;
                    }
                    add(&name);
                    if self.peek() != b'.' {
                        break;
                    }
                    self.i += 1;
                    name.push('$');
                }
                self.i += 1;
            }
            b'T' => {
                while self.i < self.s.len() && self.peek() != b';' {
                    self.i += 1;
                }
                self.i += 1;
            }
            _ => self.i += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Signature;

    fn names(s: &str) -> Vec<String> {
        let mut out = Vec::new();
        Signature { s: s.as_bytes(), i: 0 }.all(&mut |n| out.push(n.to_owned()));
        out
    }

    #[test]
    fn reads_classes_of_signatures() {
        assert_eq!(names("(La/B$C;[LD;I)V"), ["a/B$C", "D"]);
        assert_eq!(
            names("<T:Ljava/lang/Object;>(TT;)Lscala/F<TT;+LX$Y;>;"),
            ["java/lang/Object", "X$Y", "scala/F"]
        );
        assert_eq!(names("LO<LA;>.I;"), ["A", "O", "O$I"]);
    }
}
