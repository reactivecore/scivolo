// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatureVisitor.scala
//! The classes a class file refers to whose names contain `$`, the candidates for its
//! `InnerClasses` attribute (dotty's `NestedClassesCollector`).

use crate::ClassFile;
use crate::pool::Entry;

impl ClassFile {
    /// The internal names with a `$` of the classes the class, its members, their descriptors and
    /// signatures, and its code refer to, each once.
    pub fn referenced_classes(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut add = |name: String| {
            if name.contains('$') && !out.contains(&name) {
                out.push(name);
            }
        };
        let signature = self.pool.find_utf8("Signature");
        let signatures = (self.fields.iter().chain(&self.methods).flat_map(|m| &m.attributes))
            .chain(&self.attributes)
            .filter(|a| signature == Some(a.name) && a.bytes.len() == 2)
            .map(|a| u16::from_be_bytes([a.bytes[0], a.bytes[1]]));
        let members = self.fields.iter().chain(&self.methods).map(|m| m.descriptor);
        let mut texts: Vec<u16> = signatures.chain(members).collect();
        for e in self.pool.entries() {
            match *e {
                Entry::Class(n) => match self.pool.text(n) {
                    name if name.starts_with('[') => texts.push(n),
                    name => add(name),
                },
                Entry::NameAndType(_, d) | Entry::MethodType(d) => texts.push(d),
                _ => {}
            }
        }
        for t in texts {
            let text = self.pool.text(t);
            Signature { s: text.as_bytes(), i: 0 }.all(&mut add);
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

    fn all(&mut self, add: &mut impl FnMut(String)) {
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

    fn ty(&mut self, add: &mut impl FnMut(String)) {
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
                    add(name.clone());
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
        Signature { s: s.as_bytes(), i: 0 }.all(&mut |n| out.push(n));
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
