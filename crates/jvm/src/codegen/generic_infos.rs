// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//   compiler/src/dotty/tools/dotc/transform/Mixin.scala
//! The types generic signatures of members are made from (dotty's `getGenericSignature` and
//! `Mixin`'s `mixinGenericInfos`).

use super::Gen;
use super::descriptors::internal;
use super::signatures::Member;
use names::name_kinds;
use tasty::tags::PRIVATE;
use tasty::{Kind, SymbolId, TypeId};

/// A forwarder to a default method of a Java interface.
#[derive(Clone, Copy)]
struct JavaForwarded {
    /// The interface method.
    target: SymbolId,
    /// The forwarder's type seen from its class.
    info: TypeId,
}

impl Gen<'_, '_> {
    /// The generic signature of member `sym` of descriptor `desc` (dotty's `getGenericSignature`
    /// with `Mixin`'s `mixinGenericInfos`): none for an accessor of a private trait value, a member
    /// of a local class, nor for a mixin forwarder whose signature does not erase to `desc`; of a method whose context
    /// closures became parameter clauses, from its type before.
    pub fn generic_signature(&mut self, sym: SymbolId, desc: &str) -> Option<String> {
        let mixed = self.mixed_origin(sym);
        let java_forwarded = self.java_forwarded(sym);
        let origin = mixed.or(java_forwarded.map(|f| f.target)).unwrap_or(sym);
        let info = match (self.l.unflattened_info(origin), java_forwarded) {
            (Some(info), _) => info,
            (None, Some(f)) => f.info,
            (None, None) => self.seen_from_owner(sym),
        };
        if mixed.is_some_and(|o| self.l.u.symbol(o).flags.has(PRIVATE)) || self.is_local(sym) {
            return None;
        }

        let thrown = self.thrown(sym);
        let member = self.signed_member(sym, origin);
        let sig = super::signatures::member(self.l.u, member, info, desc, &thrown)?;
        let forwarder =
            self.l.mixed_in_from(sym).is_some() && !self.l.is_mixin_accessor(sym) || java_forwarded.is_some();
        (!forwarder || super::signatures::erases_to(&sig, desc)).then_some(sig)
    }

    /// Determines the member signed for `sym` forwarding to `origin`: of an extension method
    /// (dotty's `ExtensionMethods`) also its value class.
    pub fn signed_member(&mut self, sym: SymbolId, origin: SymbolId) -> Member {
        let holder = self.l.annotations_holder(sym);
        let extension = holder != sym && self.l.mixed_in_from(sym).is_none();
        let extended = extension.then(|| self.l.u.symbol(holder).owner).flatten();
        let top = self.original_top(sym);
        Member { sym, origin, extended, top }
    }

    /// Determines the top-level class `sym` is in before local classes were lifted (dotty's
    /// `topLevelClass` at erasure).
    pub fn original_top(&self, sym: SymbolId) -> SymbolId {
        let mut top = sym;
        while let Some(o) = self.l.original_owner(top).filter(|&o| self.l.u.symbol(o).kind != Kind::Package) {
            top = o;
        }
        top
    }

    /// Whether a term owns `sym` directly or through classes, before local classes were lifted
    /// (dotty's `isLocal` at erasure).
    fn is_local(&self, sym: SymbolId) -> bool {
        let mut s = sym;
        while let Some(o) = self.l.original_owner(s) {
            match self.l.u.symbol(o).kind {
                Kind::Package => return false,
                k if k.is_term() => return true,
                _ => s = o,
            }
        }
        false
    }

    /// The type of member `sym` seen from its class (dotty's `memberInfo` of the owner's
    /// `thisType`), of a mixin accessor or forwarder that of the trait member.
    pub fn seen_from_owner(&mut self, sym: SymbolId) -> TypeId {
        let owner = self.l.u.symbol(sym).owner.filter(|&o| self.l.u.is_class(o));
        let name = &self.l.u.symbol(sym).name;
        if name_kinds::is_outer(name) {
            let (_, result) = self.l.u.erased_method(sym);
            if let tasty::Erased::Class(outer) = result
                && let Some(Some(c)) = self.classes.get(&internal(&outer)).copied()
            {
                return self.l.u.this_type(c);
            }
        }
        match owner {
            Some(o) => {
                let this = self.l.u.this_type(o);
                let member = self.mixed_origin(sym).unwrap_or(sym);
                self.l.u.member_info(this, member)
            }
            None => self.l.u.info(sym),
        }
    }

    /// The forwarder `sym` is to a default method of a JDK interface its class implements, if it
    /// is one (dotty's `mkMixinForwarderSym`).
    fn java_forwarded(&mut self, sym: SymbolId) -> Option<JavaForwarded> {
        let target = self.l.jdk_forwarded_from(sym)?;
        let class = self.l.u.symbol(sym).owner?;
        let interface = self.l.u.symbol(target).owner?;
        let this = self.l.u.this_type(class);
        let info = self.l.u.info(sym);
        let info = self.l.u.as_seen_from(info, this, interface);
        Some(JavaForwarded { target, info })
    }

    /// The trait member mixin accessor or forwarder `sym` copies, or the field of a mixin getter
    /// its getter's.
    fn mixed_origin(&mut self, sym: SymbolId) -> Option<SymbolId> {
        if let Some(m) = self.l.mixed_in_from(sym) {
            return Some(m);
        }
        if self.l.u.symbol(sym).kind == Kind::Def {
            return None;
        }
        let (owner, name) = (self.l.u.symbol(sym).owner?, self.l.u.symbol(sym).name.clone());
        let getters = self.l.u.decls_named(owner, &name);
        getters.into_iter().find(|&g| g != sym).and_then(|g| self.l.mixed_in_from(g))
    }
}
