// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! The Java annotations a class file keeps (dotty's `emitAnnotations`).

use super::Gen;
use super::descriptors::{class_descriptor, class_internal};
use classfile::{ClassFile, Last};
use tasty::{Retention, SymbolId};
use typer::tpd::Tree;

/// A Java annotation a class file keeps: its class's descriptor and whether it is kept at run time.
pub struct Kept {
    descriptor: String,
    visible: bool,
}

impl Gen<'_, '_> {
    /// Determines the annotations of `sym` of Java annotation classes read from class files that
    /// its class file keeps, those whose retention is not `SOURCE` (dotty's
    /// `shouldEmitAnnotation`). Refuses, with what is not supported, such an annotation with
    /// arguments and several kept annotations.
    pub fn kept_annotations(&mut self, sym: SymbolId) -> Result<Vec<Kept>, &'static str> {
        let mut kept = Vec::new();
        for annot in self.l.tpd.annotations(sym).to_vec() {
            let tpe = self.l.tpd.tpe(annot);
            let Some(c) = self.l.u.class_symbol(tpe) else { continue };
            let Some(retention) = self.l.u.classfile_retention(c) else { continue };
            let no_args = match self.l.tpd.tree(annot) {
                Tree::Apply { args, .. } => self.l.tpd.items(*args).is_empty(),
                Tree::New { .. } => true,
                _ => false,
            };
            if !no_args {
                return Err("Java annotations with arguments on the JVM");
            }
            if retention == Retention::Source {
                continue;
            }
            let descriptor = class_descriptor(&class_internal(&self.l.u.erase(tpe)));
            kept.push(Kept { descriptor, visible: retention == Retention::Runtime });
        }
        if kept.len() > 1 {
            return Err("several Java annotations of one definition on the JVM");
        }
        Ok(kept)
    }

    /// Adds to the class `cf`, or its member added last, the Java annotations it keeps of `sym`,
    /// whose `kept_annotations` were checked.
    pub fn emit_annotations(&mut self, cf: &mut ClassFile, last: Option<Last>, sym: SymbolId) {
        let kept = self.kept_annotations(sym).unwrap_or_default();
        for visible in [true, false] {
            let descriptors: Vec<String> =
                kept.iter().filter(|k| k.visible == visible).map(|k| k.descriptor.clone()).collect();
            if !descriptors.is_empty() {
                cf.annotations(last, visible, &descriptors);
            }
        }
    }
}
