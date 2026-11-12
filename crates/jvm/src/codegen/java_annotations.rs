// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! The Java annotations a class file keeps (dotty's `emitAnnotations`).

use super::Gen;
use super::descriptors::{class_descriptor, class_internal};
use classfile::{ClassFile, Last};
use names::name_kinds;
use tasty::tags::FIELDaccessor;
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
    /// `shouldEmitAnnotation`). Refuses, with what is not supported, a kept annotation with
    /// arguments and several kept annotations.
    pub fn kept_annotations(&mut self, sym: SymbolId) -> Result<Vec<Kept>, &'static str> {
        let mut kept = Vec::new();
        for annot in self.l.tpd.annotations(sym).to_vec() {
            let tpe = self.l.tpd.tpe(annot);
            let Some(c) = self.l.u.class_symbol(tpe) else { continue };
            let Some(retention) = self.l.u.classfile_retention(c) else { continue };
            if retention == Retention::Source {
                continue;
            }
            let no_args = match self.l.tpd.tree(annot) {
                Tree::Apply { args, .. } => self.l.tpd.items(*args).is_empty(),
                Tree::New { .. } => true,
                _ => false,
            };
            if !no_args {
                return Err("Java annotations with arguments on the JVM");
            }
            let descriptor = class_descriptor(&class_internal(&self.l.u.erase(tpe)));
            kept.push(Kept { descriptor, visible: retention == Retention::Runtime });
        }
        if kept.len() > 1 {
            return Err("several Java annotations of one definition on the JVM");
        }
        Ok(kept)
    }

    /// Determines the member whose Java annotations method `m` keeps (dotty's `Memoize` and
    /// `mkForwarderSym`): none for a setter or a getter of a field, the trait member a mixin
    /// forwarder calls, else `m`.
    pub fn method_annotations_of(&self, m: SymbolId) -> Option<SymbolId> {
        let s = self.l.u.symbol(m);
        let setter = s.flags.has(FIELDaccessor) && s.name.ends_with(name_kinds::SETTER_SUFFIX);
        if setter || self.l.has_field(m) {
            return None;
        }
        Some(self.l.mixed_in_from(m).unwrap_or(m))
    }

    /// Determines the member whose Java annotations field `field` keeps (dotty's `Memoize`): the
    /// getter it was made for, or the trait member that getter forwards to; none for the storage
    /// of a lazy value.
    pub fn field_annotations_of(&self, field: SymbolId) -> Option<SymbolId> {
        if self.l.is_lazy_storage(field) {
            return None;
        }
        let getter = self.l.field_getter(field).unwrap_or(field);
        Some(self.l.mixed_in_from(getter).unwrap_or(getter))
    }

    /// Adds to method `m`, added last to `cf`, the Java annotations it keeps.
    pub fn emit_method_annotations(&mut self, cf: &mut ClassFile, m: SymbolId) {
        if let Some(holder) = self.method_annotations_of(m) {
            self.emit_annotations(cf, Some(Last::Method), holder);
        }
    }

    /// Whether a parameter of `params` has Java annotations a class file would keep.
    pub fn has_kept_param_annotations(&mut self, params: &[SymbolId]) -> bool {
        params.iter().any(|&p| self.kept_annotations(p).map_or(true, |k| !k.is_empty()))
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
