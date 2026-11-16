// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/classfile/ClassfileParser.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeUtils.scala
//! The access flags dotty gives a class it reads from a Java class file.

use super::Gen;
use classfile::{ACC_ABSTRACT, ACC_ANNOTATION, ACC_ENUM, ACC_FINAL, ACC_INTERFACE, ACC_PUBLIC};
use tasty::SymbolId;

impl Gen<'_, '_> {
    /// Determines the flags of class `d` but `ACC_STATIC` as dotty's `javaFlags` gives them to
    /// the class it reads from the Java class file `d` stands for, of the JDK, a jar or a Java
    /// source (`ClassfileParser`'s flags): an enum with constants abstract and not final, an
    /// interface abstract. None for a class of no Java class file and for an annotation
    /// interface.
    pub fn classfile_class_flags(&mut self, d: SymbolId) -> Option<u16> {
        let info = match self.l.u.jdk_class_of(d) {
            Some(info) => info,
            None if self.l.u.is_from_classfile(d) => {
                let internal = self.class_name(d);
                self.l.u.java_class_info(&internal)?
            }
            None => return None,
        };
        let access = info.access;
        if access & ACC_ANNOTATION != 0 {
            return None;
        }
        let interface = access & ACC_INTERFACE != 0;
        let enum_ = access & ACC_ENUM != 0;
        let enum_sealed = enum_ && info.fields.iter().any(|f| f.access & ACC_ENUM != 0);
        let abstract_ = interface || enum_sealed || access & ACC_ABSTRACT != 0;
        let final_ = access & ACC_FINAL != 0 && !abstract_;
        let flags = ACC_PUBLIC | if abstract_ { ACC_ABSTRACT } else { 0 };
        let flags = flags | if interface { ACC_INTERFACE } else { 0 } | if final_ { ACC_FINAL } else { 0 };
        Some(flags | if enum_ { ACC_ENUM } else { 0 })
    }
}
