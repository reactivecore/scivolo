//! NIR names of classes, modules and methods (nscplugin's `NirGenName`).

use super::Gen;
use crate::symbols::{has, is_constructor, is_java_static, is_module_class, is_private, is_trait};
use nir::GlobalId;
use nir::sig::{Scope, Sig};
use tasty::tags::{ENUM, MACRO, PRIVATE, PROTECTED, STATIC};
use tasty::{SymbolId, encode, module_class_name, module_name};

/// Classes Scala Native defines under another name, and `Nothing` and `Null`.
const MAPPED: [(&str, &str); 11] = [
    ("scala.scalanative.runtime._Class", "java.lang.Class"),
    ("scala.scalanative.runtime._Object", "java.lang.Object"),
    ("java.lang._Cloneable", "java.lang.Cloneable"),
    ("java.lang._Comparable", "java.lang.Comparable"),
    ("java.lang._Enum", "java.lang.Enum"),
    ("java.lang._NullPointerException", "java.lang.NullPointerException"),
    ("java.lang._String", "java.lang.String"),
    ("java.lang.annotation._Retention", "java.lang.annotation.Retention"),
    ("java.io._Serializable", "java.io.Serializable"),
    ("scala.Nothing", "scala.runtime.Nothing$"),
    ("scala.Null", "scala.runtime.Null$"),
];

/// Binary class name `name` as Scala Native defines it.
pub fn mapped(name: &str) -> String {
    for (from, to) in MAPPED {
        if name == from {
            return to.to_owned();
        }
        if module_name(name) == Some(from) {
            return module_class_name(to);
        }
    }
    name.to_owned()
}

impl Gen<'_, '_> {
    pub fn type_name(&mut self, c: SymbolId) -> GlobalId {
        let d = &self.l.defs;
        if [d.object, d.any, d.any_ref].contains(&c) {
            return self.pool.top("java.lang.Object");
        }
        let name = self.l.u.binary_name(c);
        self.pool.top(&mapped(&name))
    }

    pub fn module_name(&mut self, c: SymbolId) -> GlobalId {
        let name = self.type_name(c);
        let id = self.pool.top_name(name).to_owned();
        if module_name(&id).is_some() { name } else { self.pool.top(&module_class_name(&id)) }
    }

    pub fn method_name(&mut self, m: SymbolId) -> GlobalId {
        let owner_sym = self.l.u.symbol(m).owner.expect("a method's owner");
        let owner = self.type_name(owner_sym);
        let (static_, private) = (is_java_static(self.l.u, m), is_private(self.l.u, m));
        let scope = match (private, static_) {
            (true, true) => Scope::PrivateStatic(owner),
            (true, false) => Scope::Private(owner),
            (false, true) => Scope::PublicStatic,
            (false, false) => Scope::Public,
        };
        let params = self.param_types(m);
        let sig = match self.l.u.symbol(m).name.as_str() {
            _ if is_constructor(self.l.u, m) && is_trait(self.l.u, owner_sym) => {
                Sig::Method("$init$".into(), vec![self.ty(nir::Type::Unit)], scope)
            }
            _ if is_constructor(self.l.u, m) => Sig::Ctor(params),
            "<clinit>" => Sig::Clinit,
            "$init$" => Sig::Method(self.native_id(m), vec![self.ty(nir::Type::Unit)], scope),
            _ => {
                let mut types = params;
                types.push(self.result_type(m));
                Sig::Method(self.native_id(m), types, scope)
            }
        };
        self.pool.member(owner, &sig)
    }

    /// The name of the static forwarder or static method `m`, in `explicit_owner` if that
    /// derives from the owner of `m`.
    pub fn static_member_name(&mut self, m: SymbolId, explicit_owner: Option<SymbolId>) -> GlobalId {
        let owner = self.l.u.symbol(m).owner.expect("a method's owner");
        let owner =
            explicit_owner.filter(|&e| e == owner || self.l.u.derives_from(e, owner)).unwrap_or(owner);
        let type_name = self.type_name(owner);
        let no_forwarders = !has(self.l.u, m, ENUM)
            && [PRIVATE, STATIC, PROTECTED, MACRO].iter().any(|&f| has(self.l.u, m, f));
        let owner_name = if is_module_class(self.l.u, owner) && no_forwarders {
            type_name
        } else {
            let id = self.pool.top_name(type_name).to_owned();
            self.pool.top(module_name(&id).unwrap_or(&id))
        };
        let scope =
            if is_private(self.l.u, m) { Scope::PrivateStatic(owner_name) } else { Scope::PublicStatic };
        let mut types = self.param_types(m);
        types.push(self.result_type(m));
        let sig = Sig::Method(self.native_id(m), types, scope);
        self.pool.member(owner_name, &sig)
    }

    /// The simple name of local or parameter `s`.
    pub fn local_name(&self, s: SymbolId) -> String {
        encode(&self.l.u.symbol(s).name)
    }

    fn native_id(&self, m: SymbolId) -> String {
        let s = self.l.u.symbol(m);
        encode(s.target.as_deref().unwrap_or(&s.name)).replace('"', "$u0022")
    }
}
