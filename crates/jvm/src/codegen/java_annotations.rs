// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! The Java annotations a class file keeps (dotty's `emitAnnotations`).

use super::Gen;
use super::annotation_holders::Holder;
use super::descriptors::class_descriptor;
use classfile::{AnnotationValue, ClassFile, ElementValue, Last};
use tasty::{Retention, SymbolId};
use typer::tpd::TNodeId;

/// The element of a container annotation that holds the repeated annotations.
const VALUE: &str = "value";

/// The refusal of a repeated Java annotation that is not `@Repeatable` (dotty's error).
const NOT_REPEATABLE: &str = "a repeated Java annotation that is not repeatable";

/// The refusal of repeated Java annotations beside their container (dotty's error).
const CONTAINER_PRESENT: &str = "repeated Java annotations beside their container annotation";

/// A Java annotation a class file keeps: its class, its class file form and whether it is kept
/// at run time.
pub struct Kept {
    class: SymbolId,
    value: AnnotationValue,
    visible: bool,
}

impl Gen<'_, '_> {
    /// Determines the annotations of `sym` of Java annotation classes read from class files that
    /// its class file keeps, those whose retention is not `SOURCE` (dotty's
    /// `shouldEmitAnnotation`), in the order of dotty's `sym.annotations`: the reverse of the
    /// source's (`Namer.addAnnotations` prepends each), as read for a definition of TASTy; those
    /// of a class repeated replaced by one of its container at the first (`container_value`).
    /// Refuses, with what is not supported, an argument `annotation_value` refuses.
    pub fn kept_annotations(&mut self, sym: SymbolId) -> Result<Vec<Kept>, &'static str> {
        let mut annots = self.l.tpd.annotations(sym).to_vec();
        if self.l.u.symbol(sym).tree.is_none() {
            annots.reverse();
        }
        let mut groups: Vec<(SymbolId, Vec<TNodeId>)> = Vec::new();
        for annot in annots {
            let tpe = self.l.tpd.tpe(annot);
            let Some(c) = self.l.u.class_symbol(tpe) else { continue };
            if self.l.u.classfile_retention(c).is_none() {
                continue;
            }
            match groups.iter_mut().find(|(class, _)| *class == c) {
                Some((_, group)) => group.push(annot),
                None => groups.push((c, vec![annot])),
            }
        }
        let classes: Vec<String> =
            groups.iter().map(|&(c, _)| self.l.u.binary_name(c).replace('.', "/")).collect();

        let mut kept = Vec::new();
        for (class, group) in groups {
            let (retention, value) = match group[..] {
                [annot] => {
                    let retention = self.l.u.classfile_retention(class).expect("a Java annotation");
                    if retention == Retention::Source {
                        continue;
                    }
                    (retention, self.annotation_value(annot)?)
                }
                _ => match self.container_value(class, &group, &classes)? {
                    Some(kept) => kept,
                    None => continue,
                },
            };
            kept.push(Kept { class, value, visible: retention == Retention::Runtime });
        }
        Ok(kept)
    }

    /// Determines the annotation of the container class of repeated annotations `group` of
    /// `class` that replaces them, with its retention, none if it is `SOURCE` (dotty's
    /// `RepeatableAnnotations`). Refuses, as dotty does, `class` not `@Repeatable` and a container
    /// among the internal names `classes` of the annotations beside.
    fn container_value(
        &mut self,
        class: SymbolId,
        group: &[TNodeId],
        classes: &[String],
    ) -> Result<Option<(Retention, AnnotationValue)>, &'static str> {
        let container = self.l.u.repeatable_container(class).ok_or(NOT_REPEATABLE)?.to_owned();
        if classes.contains(&container) {
            return Err(CONTAINER_PRESENT);
        }
        let retention = self.l.u.annotation_retention(&container).unwrap_or(Retention::Class);
        if retention == Retention::Source {
            return Ok(None);
        }

        let mut values = Vec::new();
        for &annot in group {
            values.push(ElementValue::Annotation(self.annotation_value(annot)?));
        }
        let elements = vec![(VALUE.to_owned(), ElementValue::Array(values))];
        Ok(Some((retention, AnnotationValue { descriptor: class_descriptor(&container), elements })))
    }

    /// Adds to method `m`, added last to `cf`, the Java annotations it keeps.
    pub fn emit_method_annotations(&mut self, cf: &mut ClassFile, m: SymbolId) {
        if let Some(holder) = self.method_annotations_of(m) {
            self.emit_annotations(cf, Some(Last::Method), holder);
        }
    }

    /// Adds to the method added last to `cf` the Java annotations of `holders`, its parameters'
    /// `param_holders` (dotty's `emitParamAnnotations`).
    pub fn emit_param_annotations(&mut self, cf: &mut ClassFile, holders: &[Holder]) {
        let kept: Vec<Vec<Kept>> = holders.iter().map(|&h| self.kept_of(h)).collect();
        if kept.iter().all(Vec::is_empty) {
            return;
        }
        for visible in [true, false] {
            let values: Vec<Vec<AnnotationValue>> = kept.iter().map(|k| of_visibility(k, visible)).collect();
            if values.iter().any(|v| !v.is_empty()) {
                cf.parameter_annotations(visible, &values);
            }
        }
    }

    /// Adds to the class `cf`, or its member added last, the Java annotations of `holder` it
    /// keeps, whose `kept_annotations` were checked.
    pub fn emit_annotations(&mut self, cf: &mut ClassFile, last: Option<Last>, holder: Holder) {
        let kept = self.kept_of(holder);
        for visible in [true, false] {
            let values = of_visibility(&kept, visible);
            if !values.is_empty() {
                cf.annotations(last, visible, &values);
            }
        }
    }

    /// Determines the Java annotations of `holder` its member keeps, in dotty's order: those
    /// `meant_for` it, reversed once more if copied; none for one `kept_annotations` refuses.
    fn kept_of(&mut self, holder: Holder) -> Vec<Kept> {
        let mut kept = self.kept_annotations(holder.sym).unwrap_or_default();
        if let Some(meant) = self.meant_for(holder) {
            kept.retain(|k| meant.contains(&k.class));
        }
        if holder.copied {
            kept.reverse();
        }
        kept
    }
}

fn of_visibility(kept: &[Kept], visible: bool) -> Vec<AnnotationValue> {
    kept.iter().filter(|k| k.visible == visible).map(|k| k.value.clone()).collect()
}
