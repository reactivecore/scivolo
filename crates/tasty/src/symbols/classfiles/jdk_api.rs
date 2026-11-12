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

    /// Selects among members `found` of class `owner` those `jdk_lacks` finds, each member
    /// checked once: methods and values, not objects nor members entered from class files. A
    /// method the JDK has, but a constructor, takes its type from there (`jdk_info`) where both
    /// have as many parameter clauses (javalib's `def pattern: String` stays parameterless).
    pub fn jdk_lacking(&mut self, owner: SymbolId, found: &[SymbolId]) -> Vec<SymbolId> {
        let fresh: Vec<SymbolId> =
            found.iter().copied().filter(|&m| self.jdk_checked.insert(m) && self.is_api_term(m)).collect();
        if fresh.is_empty() {
            return fresh;
        }
        let Some(info) = self.jdk_class_of(owner) else { return Vec::new() };
        let mut lacking = Vec::new();
        for m in fresh {
            let Some(member) = declaration(self, m, &info) else {
                lacking.push(m);
                continue;
            };
            let method = self.symbol(m).kind == Kind::Def && member.name != nme::CONSTRUCTOR;
            if method && let Some(t) = jdk_type(self, m, member, &info) {
                let own = self.info(m);
                if self.param_clauses(own).len() == self.param_clauses(t).len() {
                    self.infos.insert(m, t);
                }
            }
        }
        lacking
    }

    /// Whether `m` is a method or a value of a class's TASTy, neither an object, a trait's
    /// constructor nor `String.+`, which dotty adds to the JDK's `String`.
    fn is_api_term(&self, m: SymbolId) -> bool {
        let s = self.symbol(m);
        let term = s.kind == Kind::Def || s.kind == Kind::Val && !s.flags.has(OBJECT);
        let trait_init = s.name == nme::CONSTRUCTOR && s.owner.is_some_and(|o| self.is_trait(o));
        let string_add = s.name == nme::ADD && s.owner == Some(self.defn().string);
        term && !trait_init && !string_add && !self.classfile_descriptors.contains_key(&m)
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
        jdk_type(self, m, member, &info)
    }
}

/// The type of member `m` of a library class as `member` of its JDK class file `info` declares
/// it; none if the signature is refused.
fn jdk_type(u: &mut Universe, m: SymbolId, member: &MemberInfo, info: &ClassInfo) -> Option<TypeId> {
    let owner = u.symbol(m).owner?;
    let statics = u.symbol(owner).flags.has(OBJECT);
    let owners = library_owners(u, owner, statics, info)?;
    let text = member.signature.as_deref().unwrap_or(&member.descriptor);
    match u.symbol(m).kind {
        Kind::Def => method_type(u, &owners, member, text).ok(),
        _ => Sig::new(text).ty(u, &owners.vars).ok(),
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
