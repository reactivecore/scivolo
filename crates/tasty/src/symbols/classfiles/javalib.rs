//! The members of a JDK class file that the library's class of the same name lacks, entered into
//! that class (ADR 0108).

use super::super::{Kind, SymbolId, Universe};
use super::class::{Owners, member};
use super::signatures::{Sig, bounded_by_object};
use super::{annotation, classpath};
use crate::tags::OBJECT;
use crate::types::Type;
use classfile::{ACC_ANNOTATION, ACC_PRIVATE, ACC_PROTECTED, ACC_PUBLIC, ACC_STATIC, ClassInfo, MemberInfo};
use names::nme;
use rustc_hash::FxHashSet;
use std::rc::Rc;

impl Universe {
    /// Enters into class `c`, a `java.*` or `javax.*` class of the library other than `Object`,
    /// the members of its JDK class file named as no library member of `c` is, its base classes'
    /// first, once per class: an object's class gets the static members, a class the instance
    /// ones; package-private ones are left out. A name of which a member is refused is not
    /// entered.
    pub fn enter_jdk_members(&mut self, c: SymbolId) {
        if !self.jdk_member_classes.insert(c) {
            return;
        }
        let Some(info) = self.jdk_class_of(c) else { return };
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
        self.enter_jdk_where(c, &info, |m| !present.contains(&m.name));
    }

    /// Enters into class `c` the members named `name` of its JDK class file, as
    /// `enter_jdk_members` does, once the library's members of that name are hidden.
    pub fn enter_jdk_named(&mut self, c: SymbolId, name: &str) {
        let Some(info) = self.jdk_class_of(c) else { return };
        self.enter_jdk_where(c, &info, |m| m.name == name);
    }

    /// Enters into library class `c` (an object's class for a static one) the public or protected
    /// class of its JDK class file nested in it and named `name` (`C` or its object `C$`), as
    /// `Runtime.Version`, if `c` lacks it: the declarations named `name` entered.
    pub fn enter_jdk_nested(&mut self, c: SymbolId, name: &str) -> Vec<SymbolId> {
        let Some(info) = self.jdk_class_of(c) else { return Vec::new() };
        let statics = self.symbol(c).flags.has(OBJECT);
        let simple = name.strip_suffix('$').unwrap_or(name);
        let nested = info.inner_classes.iter().find(|n| {
            n.outer.as_deref() == Some(info.name.as_str())
                && n.name.as_deref() == Some(simple)
                && (n.flags & ACC_STATIC != 0) == statics
                && n.flags & (ACC_PUBLIC | ACC_PROTECTED) != 0
        });
        let Some(nested) = nested.cloned() else { return Vec::new() };
        let Some(inner) = self.jdk.as_mut().and_then(|j| j.class(&nested.inner).cloned()) else {
            return Vec::new();
        };
        let access = inner.access & !(ACC_PUBLIC | ACC_PROTECTED | ACC_PRIVATE) | nested.flags;
        let entered = classpath::enter(self, c, simple, &inner, access);
        entered.into_iter().filter(|&d| self.symbol(d).name == name).collect()
    }

    /// Enters, once, into library class `c` the constructor and retention of its JDK class file
    /// if that is an annotation class (`annotation::enter`), the library's constructors removed:
    /// whether it is one.
    pub fn enter_jdk_annotation(&mut self, c: SymbolId) -> bool {
        if self.classfile_annotations.contains_key(&c) {
            return true;
        }
        let Some(info) = self.jdk_class_of(c).filter(|i| i.access & ACC_ANNOTATION != 0) else {
            return false;
        };
        let decls = self.symbol(c).decls.clone();
        let constructors = decls.into_iter().filter(|&d| self.symbol(d).name == nme::CONSTRUCTOR);
        constructors.collect::<Vec<_>>().into_iter().for_each(|m| self.unenter(m));
        self.enter_jdk_members(c);
        let members = self.symbol(c).decls.clone();
        annotation::enter(self, c, &info, &members);
        true
    }

    /// Enters into class `c` the members of its JDK class file `info` that `keep` selects and
    /// `c` does not have yet.
    fn enter_jdk_where(&mut self, c: SymbolId, info: &ClassInfo, keep: impl Fn(&MemberInfo) -> bool) {
        let statics = self.symbol(c).flags.has(OBJECT);
        let Some(owners) = library_owners(self, c, statics, info) else { return };
        let members = info.fields.iter().map(|f| (f, false)).chain(info.methods.iter().map(|m| (m, true)));
        let visible = |m: &MemberInfo| m.access & (ACC_PUBLIC | ACC_PROTECTED) != 0;
        let wanted: Vec<_> = members
            .filter(|(m, _)| {
                visible(m) && (m.access & ACC_STATIC != 0) == statics && keep(m) && !self.has_jdk_member(c, m)
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

    /// Whether class `c` has a member entered from JDK class file member `m`.
    fn has_jdk_member(&self, c: SymbolId, m: &MemberInfo) -> bool {
        let entered = |d: &SymbolId| self.classfile_descriptors.get(d) == Some(&m.descriptor);
        self.symbol(c).decls.iter().any(|d| self.symbol(*d).name == m.name && entered(d))
    }

    /// The JDK class file of class `c` if the universe has a JDK and `c` is a `java.*` or
    /// `javax.*` class (or object's class) of the library other than `Object`.
    pub fn jdk_class_of(&mut self, c: SymbolId) -> Option<Rc<ClassInfo>> {
        let s = self.symbol(c);
        if self.jdk.is_none() || self.javalib_sources || s.kind != Kind::Class || s.tree.is_none() {
            return None;
        }
        if let Some(known) = self.jdk_classes.get(&c) {
            return known.clone();
        }
        let found = self.read_jdk_class_of(c);
        self.jdk_classes.insert(c, found.clone());
        found
    }

    /// The JDK class file of library class `c` if `c` is named as a `java.*` or `javax.*` class
    /// (or object's class) other than `Object`.
    fn read_jdk_class_of(&mut self, c: SymbolId) -> Option<Rc<ClassInfo>> {
        let statics = self.symbol(c).flags.has(OBJECT);
        if !self.full_name_starts_with(c, "java.") && !self.full_name_starts_with(c, "javax.") {
            return None;
        }
        let full = self.full_name(c);
        let full = if statics { full.strip_suffix('$').unwrap_or(&full) } else { &full };
        let java = full.starts_with("java.") || full.starts_with("javax.");
        if !java || full == "java.lang.Object" {
            return None;
        }
        let internal = full.replace("$.", "$").replace('.', "/");
        self.jdk.as_mut()?.class(&internal).cloned()
    }
}

/// Library class `c` (an object's class if `statics`) as the owner of the members of its JDK
/// class `info`: the JDK class's type parameters stand for those of `c` (`C.this.A`), by
/// position; none if their counts differ or the signature is refused.
pub fn library_owners<'a>(
    u: &mut Universe,
    c: SymbolId,
    statics: bool,
    info: &'a ClassInfo,
) -> Option<Owners<'a>> {
    let mut owners = Owners { class: c, statics: c, params: Vec::new(), vars: Vec::new(), outer: 0 };
    let Some(signature) = info.signature.as_deref().filter(|_| !statics) else { return Some(owners) };
    let params = Sig::new(signature).type_params(u).ok()?;
    let syms = u.type_params(c);
    if params.len() != syms.len() {
        return None;
    }
    let class = u.types.intern(Type::Sym(None, c));
    let this = u.types.intern(Type::This(class));
    for ((name, b), p) in params.into_iter().zip(syms) {
        let t = u.types.intern(Type::Sym(Some(this), p));
        owners.vars.push((name.clone(), t, bounded_by_object(b)));
        owners.params.push((name, b));
    }
    Some(owners)
}
