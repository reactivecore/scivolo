//! The members of a JDK class file that the library's class of the same name lacks, entered into
//! that class (ADR 0108).

use super::super::{Kind, SymbolId, Universe};
use super::class::{Owners, member};
use super::signatures::{Sig, bounded_by_object};
use crate::tags::OBJECT;
use crate::types::Type;
use classfile::{ACC_PROTECTED, ACC_PUBLIC, ACC_STATIC, MemberInfo};
use rustc_hash::FxHashSet;

impl Universe {
    /// Enters into class `c`, a `java.*` or `javax.*` class of the library other than `Object`,
    /// the members of its JDK class file named as no library member of `c` is, its base classes'
    /// first, once per class: an object's class gets the static members, a class the instance
    /// ones; package-private ones are left out. A name of which a member is refused is not
    /// entered.
    pub fn enter_jdk_members(&mut self, c: SymbolId) {
        if self.jdk.is_none() || self.javalib_sources || !self.jdk_member_classes.insert(c) {
            return;
        }
        let s = self.symbol(c);
        if s.kind != Kind::Class || s.tree.is_none() {
            return;
        }
        let statics = s.flags.has(OBJECT);
        let full = self.full_name(c);
        let full = if statics { full.strip_suffix('$').unwrap_or(&full) } else { &full };
        let java = full.starts_with("java.") || full.starts_with("javax.");
        if !java || full == "java.lang.Object" {
            return;
        }
        let internal = full.replace("$.", "$").replace('.', "/");
        let Some(info) = self.jdk.as_mut().and_then(|jdk| jdk.class(&internal)).cloned() else { return };
        let bases = self.linearization(c);
        for &base in &bases[1..] {
            self.enter_jdk_members(base);
        }
        let present: FxHashSet<String> = bases
            .iter()
            .flat_map(|&b| self.symbol(b).decls.iter())
            .filter(|d| !self.classfile_descriptors.contains_key(d))
            .map(|&d| self.symbol(d).name.clone())
            .collect();
        let mut owners = Owners { class: c, statics: c, params: Vec::new(), vars: Vec::new(), outer: 0 };
        if !statics && let Some(signature) = info.signature.as_deref() {
            let Ok(params) = Sig::new(signature).type_params(self) else { return };
            let syms = self.type_params(c);
            if params.len() != syms.len() {
                return;
            }
            let class = self.types.intern(Type::Sym(None, c));
            let this = self.types.intern(Type::This(class));
            for ((name, b), p) in params.into_iter().zip(syms) {
                let t = self.types.intern(Type::Sym(Some(this), p));
                owners.vars.push((name.clone(), t, bounded_by_object(b)));
                owners.params.push((name, b));
            }
        }
        let members = info.fields.iter().map(|f| (f, false)).chain(info.methods.iter().map(|m| (m, true)));
        let visible = |m: &MemberInfo| m.access & (ACC_PUBLIC | ACC_PROTECTED) != 0;
        let wanted: Vec<_> = members
            .filter(|(m, _)| {
                visible(m) && (m.access & ACC_STATIC != 0) == statics && !present.contains(&m.name)
            })
            .collect();
        let mut entered: Vec<SymbolId> = Vec::new();
        let mut refused: FxHashSet<&str> = FxHashSet::default();
        for &(m, method) in &wanted {
            match member(self, &owners, m, method) {
                Ok(Some(s)) => entered.push(s),
                Ok(None) => {}
                Err(_) => _ = refused.insert(&m.name),
            }
        }
        for s in entered {
            if refused.contains(self.symbol(s).name.as_str()) {
                self.unenter(s);
            }
        }
    }
}
