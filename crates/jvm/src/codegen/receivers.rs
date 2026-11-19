// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! The class a virtual call or a field access names (dotty's `receiverClass` of `genApply` and
//! `genCallMethod`).

use super::callee::Receiver;
use super::descriptors::{descriptor, internal, method_descriptor};
use super::{Gen, is_static_member};
use names::nme;
use tasty::{Erased, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// `Object`'s methods a trait may override, with their descriptors.
const OBJECT_METHODS: [(&str, &str); 5] = [
    (nme::EQUALS, "(Ljava/lang/Object;)Z"),
    (nme::HASH_CODE, "()I"),
    (nme::TO_STRING, "()Ljava/lang/String;"),
    (nme::CLONE, "()Ljava/lang/Object;"),
    (nme::FINALIZE, "()V"),
];

impl Gen<'_, '_> {
    /// Determines whether member `m`, a getter's call if `called`, is reached without a receiver:
    /// a member of an object standing for a JDK class's statics, or a static member but a getter.
    pub fn is_static_access(&self, m: SymbolId, called: bool) -> bool {
        let owner = self.l.u.symbol(m).owner.expect("a member's owner");
        let getter = called && self.l.u.symbol(m).kind == tasty::Kind::Val;
        lower::is_jdk_object(self.l.u, owner) || (is_static_member(self.l, m) && !getter)
    }

    /// Determines whether a call of method `m` on `qualifier` loads its receiver as dotty's
    /// `genLoadQualifier`: of an instance method, but on `new` or `super`.
    pub fn loads_receiver(&self, m: SymbolId, qualifier: Option<TNodeId>) -> bool {
        let made =
            matches!(qualifier.map(|q| self.l.tpd.tree(q)), Some(Tree::New { .. } | Tree::Super { .. }));
        !made && !self.is_static_access(m, true)
    }

    /// The class a virtual call of `m` on `qualifier` names: the qualifier's erased class (its
    /// class for `super`), `Object` or the array type for an array, `m`'s trait for a trait method
    /// overriding one of `Object`; none where the qualifier's type erases to no class of its own or
    /// to one not deriving from `m`'s owner, as a by-name parameter's.
    pub fn receiver_class(&mut self, qualifier: TNodeId, m: SymbolId) -> Option<Receiver> {
        let owner = self.l.u.symbol(m).owner?;
        let t = self.l.tpd.tpe(qualifier);
        let t = self.l.u.widen(t);
        let erased = match self.l.tpd.tree(qualifier) {
            Tree::Super { .. } => self.l.u.erase(t),
            _ => lower::erased_type(self.l, qualifier),
        };
        match erased {
            e @ Erased::Array(_) if self.l.u.symbol(m).name == nme::CLONE => {
                Some(Receiver::Array(descriptor(&e)))
            }
            Erased::Array(_) => Some(Receiver::Array("java/lang/Object".into())),
            Erased::Class(name) => {
                let c = match self.l.u.class_symbol(t) {
                    Some(c) => c,
                    None => self.resolve_class(&internal(&name))?,
                };
                let c = match self.l.u.binary_name(c) == name {
                    true => c,
                    false => {
                        self.l.u.linearization(c).into_iter().find(|&b| self.l.u.binary_name(b) == name)?
                    }
                };
                if !self.l.u.derives_from(c, owner) {
                    return None;
                }
                let trait_override = c != owner && self.l.u.is_trait(c) && self.l.u.is_trait(owner);
                Some(Receiver::Class(if trait_override && self.overrides_object(m) { owner } else { c }))
            }
            _ => None,
        }
    }

    /// Whether method `m` overrides a method of `Object` with its erased signature, as a method
    /// whose result erases to another class than `Object`'s does not.
    fn overrides_object(&mut self, m: SymbolId) -> bool {
        let name = self.l.u.symbol(m).name.clone();
        let Some(&(_, desc)) = OBJECT_METHODS.iter().find(|o| o.0 == name) else { return false };
        method_descriptor(self.l.u, m) == desc
    }
}
