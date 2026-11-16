// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//   compiler/src/dotty/tools/dotc/transform/Memoize.scala
//! The Java annotations a class file keeps (dotty's `emitAnnotations`).

use super::Gen;
use classfile::{AnnotationValue, ClassFile, Last};
use names::name_kinds;
use tasty::defn::{META_TARGETS, NON_BEAN_TARGETS};
use tasty::tags::{FIELDaccessor, PARAMsetter, SYNTHETIC};
use tasty::{Kind, Retention, SymbolId};

/// The meta-annotation target of fields, `scala.annotation.meta.field`.
const FIELD: &[&str] = &["field"];

/// The meta-annotation targets of getters and fields.
const GETTER_FIELD: &[&str] = &["getter", "field"];

/// The meta-annotation target of parameters.
const PARAM: &[&str] = &["param"];

/// A Java annotation a class file keeps: its class, its class file form and whether it is kept
/// at run time.
pub struct Kept {
    class: SymbolId,
    value: AnnotationValue,
    visible: bool,
}

/// The definition whose Java annotations a class file member keeps, whether dotty copied them
/// an odd number of times to the member (`addAnnotations`, which prepends each, so reverses
/// them), and whether a field took them from its getter.
#[derive(Clone, Copy)]
pub struct Holder {
    pub sym: SymbolId,
    copied: bool,
    field: bool,
}

impl Holder {
    /// Makes the holder of the annotations of `sym` itself.
    pub fn own(sym: SymbolId) -> Holder {
        Holder { sym, copied: false, field: false }
    }

    fn copied_from(self, sym: SymbolId) -> Holder {
        Holder { sym, copied: !self.copied, ..self }
    }
}

impl Gen<'_, '_> {
    /// Determines the annotations of `sym` of Java annotation classes read from class files that
    /// its class file keeps, those whose retention is not `SOURCE` (dotty's
    /// `shouldEmitAnnotation`), in the order of dotty's `sym.annotations`: the reverse of the
    /// source's (`Namer.addAnnotations` prepends each), as read for a definition of TASTy.
    /// Refuses, with what is not supported, an argument `annotation_value` refuses.
    pub fn kept_annotations(&mut self, sym: SymbolId) -> Result<Vec<Kept>, &'static str> {
        let mut kept = Vec::new();
        for annot in self.l.tpd.annotations(sym).to_vec() {
            let tpe = self.l.tpd.tpe(annot);
            let Some(c) = self.l.u.class_symbol(tpe) else { continue };
            let Some(retention) = self.l.u.classfile_retention(c) else { continue };
            if retention == Retention::Source {
                continue;
            }
            let value = self.annotation_value(annot)?;
            kept.push(Kept { class: c, value, visible: retention == Retention::Runtime });
        }
        if self.l.u.symbol(sym).tree.is_none() {
            kept.reverse();
        }
        Ok(kept)
    }

    /// Determines the member whose Java annotations method `m` keeps (dotty's `Memoize` and
    /// `mkForwarderSym`): none for a setter or a getter of a field, the trait member a mixin
    /// forwarder calls, else `m`.
    pub fn method_annotations_of(&self, m: SymbolId) -> Option<Holder> {
        let s = self.l.u.symbol(m);
        let setter = s.flags.has(FIELDaccessor) && s.name.ends_with(name_kinds::SETTER_SUFFIX);
        if setter || self.l.has_field(m) {
            return None;
        }
        Some(self.mixed_in(Holder::own(m)))
    }

    /// Determines the member whose Java annotations field `field` keeps (dotty's `Memoize`): the
    /// getter it was made for, or the trait member that getter forwards to; none for the storage
    /// of a lazy value.
    pub fn field_annotations_of(&self, field: SymbolId) -> Option<Holder> {
        if self.l.is_lazy_storage(field) {
            return None;
        }
        let own = Holder::own(field);
        let from_getter = |getter| Holder { field: true, ..own.copied_from(getter) };
        let holder = self.l.field_getter(field).map_or(own, from_getter);
        Some(self.mixed_in(holder))
    }

    fn mixed_in(&self, holder: Holder) -> Holder {
        self.l.mixed_in_from(holder.sym).map_or(holder, |from| holder.copied_from(from))
    }

    /// Adds to method `m`, added last to `cf`, the Java annotations it keeps.
    pub fn emit_method_annotations(&mut self, cf: &mut ClassFile, m: SymbolId) {
        if let Some(holder) = self.method_annotations_of(m) {
            self.emit_annotations(cf, Some(Last::Method), holder);
        }
    }

    /// Determines the holders of the Java annotations of the parameters `params` of method `m`:
    /// the parameters themselves, or those of the trait member a mixin forwarder calls
    /// (`mkForwarderSym`). Refuses, with what is not supported, an annotation argument
    /// `kept_annotations` refuses, and a forwarder whose parameters do not match its member's.
    pub fn param_holders(&mut self, m: SymbolId, params: &[SymbolId]) -> Result<Vec<Holder>, &'static str> {
        let own: Vec<Holder> = params.iter().map(|&p| Holder::own(p)).collect();
        let holders = match self.l.mixed_in_from(m) {
            Some(from) => {
                let decls = self.l.u.symbol(from).decls.clone();
                let from_params = decls.into_iter().filter(|&d| self.l.u.symbol(d).kind == Kind::Param);
                let copied: Vec<Holder> = own.iter().zip(from_params).map(|(h, p)| h.copied_from(p)).collect();
                match copied.len() == own.len() {
                    true => copied,
                    false if self.l.u.symbol(from).decls.iter().all(|&d| self.l.tpd.annotations(d).is_empty()) => own,
                    false => return Err("Java annotations of parameters of a forwarder of another arity"),
                }
            }
            None => own,
        };
        for h in &holders {
            self.kept_annotations(h.sym)?;
        }
        Ok(holders)
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

    /// Determines the Java annotations of `holder` its member keeps, in dotty's order: those
    /// meant for it (`PostTyper`'s `keepAnnotationsCarrying`, `Memoize`'s for a field), reversed
    /// once more if copied; none for one whose `kept_annotations` refuses.
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

    /// The annotation classes of `holder` meant for its member if not all are: of a field those
    /// for fields or, but of a class parameter's, without a target; of a `val` those for getters
    /// or fields or, but of a class parameter's, without a target; of a parameter those for
    /// parameters or without a target.
    fn meant_for(&mut self, holder: Holder) -> Option<Vec<SymbolId>> {
        let s = self.l.u.symbol(holder.sym);
        let accessor = s.flags.has(PARAMsetter);
        let without = |all: &'static [&'static str]| if accessor { &[][..] } else { all };
        let (targets, none_of): (&[&str], &[&str]) = match s.kind {
            Kind::Val if holder.field => (FIELD, without(META_TARGETS)),
            Kind::Val if s.flags.has(SYNTHETIC) => return None,
            Kind::Val => (GETTER_FIELD, without(NON_BEAN_TARGETS)),
            Kind::Param => (PARAM, NON_BEAN_TARGETS),
            _ => return None,
        };
        Some(self.l.u.annotations_carrying(holder.sym, targets, none_of))
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
}

fn of_visibility(kept: &[Kept], visible: bool) -> Vec<AnnotationValue> {
    kept.iter().filter(|k| k.visible == visible).map(|k| k.value.clone()).collect()
}
