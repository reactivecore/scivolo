//! The members of the library's JDK classes as the JDK's class files declare them (ADR 0118).

use super::super::{Kind, SymbolId, Universe};
use super::class::method_type;
use super::descriptor_erasure;
use super::javalib::library_owners;
use super::signatures::Sig;
use crate::tags::OBJECT;
use crate::types::TypeId;
use classfile::{ACC_BRIDGE, ACC_PROTECTED, ACC_PUBLIC, ACC_STATIC, ACC_SYNTHETIC, ClassInfo, MemberInfo};
use names::nme;

impl Universe {
    /// Determines whether member `m` of a `java.*` or `javax.*` class of the library is missing
    /// from that class's JDK class file, as javalib's helper `Map.usingEntry` is: no public or
    /// protected member there has its name, staticness and erasure. False without a JDK or
    /// without the class file.
    pub fn jdk_lacks(&mut self, m: SymbolId) -> bool {
        let Some(owner) = self.symbol(m).owner else { return false };
        let Some(info) = self.jdk_class_of(owner) else { return false };
        declaration(self, m, &info).is_none()
    }

    /// Reads the type of member `m` of a `java.*` or `javax.*` class of the library from that
    /// class's JDK class file (`Comparator.thenComparing[U <: Comparable[? >: U]]` where javalib
    /// has `U <: Comparable[U]`), the JDK class's type parameters standing for the library
    /// class's; none without a JDK, the class file or the member, or if its signature is
    /// refused.
    pub fn jdk_info(&mut self, m: SymbolId) -> Option<TypeId> {
        let owner = self.symbol(m).owner?;
        let info = self.jdk_class_of(owner)?;
        let member = declaration(self, m, &info)?;
        let statics = self.symbol(owner).flags.has(OBJECT);
        let owners = library_owners(self, owner, statics, &info)?;
        let text = member.signature.as_deref().unwrap_or(&member.descriptor);
        match self.symbol(m).kind {
            Kind::Def => method_type(self, &owners, member, text).ok(),
            _ => Sig::new(text).ty(self, &owners.vars).ok(),
        }
    }
}

/// The member of JDK class `info` that member `m` of its library class stands for: public or
/// protected, neither synthetic nor a bridge, of the name, staticness and erasure of `m`.
fn declaration<'a>(u: &mut Universe, m: SymbolId, info: &'a ClassInfo) -> Option<&'a MemberInfo> {
    let s = u.symbol(m);
    let name = s.name.clone();
    let method = s.kind == Kind::Def;
    let statics = s.owner.is_some_and(|o| u.symbol(o).flags.has(OBJECT));
    let erased = method.then(|| u.erased_method(m));
    let members = if method { &info.methods } else { &info.fields };
    members.iter().find(|d| {
        let visible = d.access & (ACC_PUBLIC | ACC_PROTECTED) != 0;
        let artifact = d.access & (ACC_SYNTHETIC | if method { ACC_BRIDGE } else { 0 }) != 0;
        let same_erasure = erased.as_ref().is_none_or(|(params, result)| {
            descriptor_erasure(&d.descriptor)
                .is_some_and(|(ps, r)| ps == *params && (name == nme::CONSTRUCTOR || r == *result))
        });
        d.name == name && visible && !artifact && (d.access & ACC_STATIC != 0) == statics && same_erasure
    })
}
