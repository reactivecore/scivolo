// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//   compiler/src/dotty/tools/dotc/transform/Memoize.scala
//   compiler/src/dotty/tools/dotc/transform/MixinOps.scala
//! The definitions whose Java annotations class file members keep, and which of them.

use super::Gen;
use names::name_kinds;
use tasty::defn::{META_TARGETS, NON_BEAN_TARGETS};
use tasty::tags::{FIELDaccessor, PARAMsetter, SYNTHETIC};
use tasty::{Kind, SymbolId};

/// The meta-annotation target of fields, `scala.annotation.meta.field`.
const FIELD: &[&str] = &["field"];

/// The meta-annotation targets of getters and fields.
const GETTER_FIELD: &[&str] = &["getter", "field"];

/// The meta-annotation target of parameters.
const PARAM: &[&str] = &["param"];

/// The definition whose Java annotations a class file member keeps, whether dotty copied them
/// an odd number of times to the member (`addAnnotations`, which prepends each, so reverses
/// them), and whether a field took them from its getter.
#[derive(Clone, Copy)]
pub struct Holder {
    pub sym: SymbolId,
    pub copied: bool,
    pub field: bool,
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

    /// Determines the holders of the Java annotations of the parameters `params` of method `m`:
    /// the parameters themselves, or those of the trait member a mixin forwarder calls
    /// (`mkForwarderSym`). Refuses, with what is not supported, an annotation argument
    /// `kept_annotations` refuses, and a forwarder whose parameters do not match its member's.
    pub fn param_holders(&mut self, m: SymbolId, params: &[SymbolId]) -> Result<Vec<Holder>, &'static str> {
        let own: Vec<Holder> = params.iter().map(|&p| Holder::own(p)).collect();
        let mut holders = own.clone();
        if let Some(from) = self.l.mixed_in_from(m) {
            let decls = self.l.u.symbol(from).decls.clone();
            let from_params: Vec<SymbolId> =
                decls.into_iter().filter(|&d| self.l.u.symbol(d).kind == Kind::Param).collect();
            if from_params.len() == own.len() {
                holders = own.iter().zip(from_params).map(|(h, p)| h.copied_from(p)).collect();
            } else if from_params.iter().any(|&p| !self.l.tpd.annotations(p).is_empty()) {
                return Err("Java annotations of parameters of a forwarder of another arity");
            }
        }
        for h in &holders {
            self.kept_annotations(h.sym)?;
        }
        Ok(holders)
    }

    /// Determines the annotation classes of `holder` meant for its member if not all are
    /// (`PostTyper`'s and `Memoize`'s `keepAnnotationsCarrying`): of a field those for fields
    /// or, but of a class parameter's, without a target; of a `val` those for getters or fields
    /// or, but of a class parameter's, without a target; of a parameter those for parameters or
    /// without a target.
    pub fn meant_for(&mut self, holder: Holder) -> Option<Vec<SymbolId>> {
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
}
