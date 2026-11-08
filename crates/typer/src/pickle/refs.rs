// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! References to symbols and named members (dotty's `NamedType` case of `pickleNewType`).

use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use crate::tpd::TNodeId;
use tasty::tags::*;
use tasty::write::Name;
use tasty::{Flags, Kind, SymbolId, Type, TypeId};

/// The prefix of a reference: a type, the `this` of a package, or the companion object of a
/// Java class of the JDK, which holds its static members.
#[derive(Clone, Copy)]
enum Prefix {
    Type(TypeId),
    PackageThis(SymbolId),
    JavaStatics(SymbolId),
}

impl TreePickler<'_> {
    /// A reference to `s` through `prefix`, a member of a class or package without one through
    /// its owner's `this`.
    pub fn sym_ref(&mut self, prefix: Option<TypeId>, s: SymbolId) -> Result<(), Refusal> {
        let sym = self.u.symbol(s);
        let (is_type, owner) = (sym.kind.is_type(), sym.owner);
        if sym.kind == Kind::Package {
            self.package_ref(s);
            return Ok(());
        }
        let owner_kind = owner.map(|o| (o, self.u.symbol(o).kind));
        let member = matches!(owner_kind, Some((_, Kind::Package | Kind::Class)));
        let root = self.u.root();
        let local = owner_kind.is_some_and(|(_, k)| k.is_term());
        let prefix = match (prefix, owner_kind) {
            (_, _) if local && self.file.is_some() && self.is_defined_here(s) => return self.direct_ref(s),
            (Some(p), _) if matches!(*self.u.types().get(p), Type::Package(r) if r == root) => {
                Prefix::PackageThis(root)
            }
            (Some(p), _) => Prefix::Type(p),
            (None, _) if self.is_defined_here(s) && (self.file.is_none() || !member) => {
                return self.direct_ref(s);
            }
            (None, Some((o, Kind::Package))) => Prefix::PackageThis(o),
            (None, Some((o, Kind::Class))) => {
                let class = type_ref(self.u, o);
                Prefix::Type(self.u.intern_type(Type::This(class)))
            }
            _ => return self.direct_ref(s),
        };
        let prefix = match prefix {
            Prefix::Type(p) => self.java_statics_ref(p, s),
            p => p,
        };
        if self.is_locally_defined(s) {
            self.byte(if is_type { TYPEREFsymbol } else { TERMREFsymbol });
            self.sym_addr(s);
            return self.prefix(prefix);
        }
        let owner = owner.ok_or("a reference to a symbol without owner")?;
        let shadowed = match prefix {
            Prefix::Type(p) if self.u.symbol(s).kind == Kind::Class => {
                let name = self.u.symbol(s).name.clone();
                self.u.resolve(p, &name, true) != Some(s)
            }
            _ => false,
        };
        if self.u.symbol(s).flags.has(PRIVATE) || shadowed {
            let owner_ref = type_ref(self.u, owner);
            return self.with_length(if is_type { TYPEREFin } else { TERMREFin }, &mut |p| {
                p.signed_name(s, !is_type)?;
                p.prefix(prefix)?;
                p.ty(owner_ref, false)
            });
        }
        self.byte(if is_type { TYPEREF } else { TERMREF });
        self.signed_name(s, !is_type)?;
        self.prefix(prefix)
    }

    /// Prefix `p` of static member `s` of a Java class, the class's `this` as a reference to its
    /// companion object.
    fn java_statics_ref(&mut self, p: TypeId, s: SymbolId) -> Prefix {
        let Type::This(class) = *self.u.types().get(p) else { return Prefix::Type(p) };
        let Some(c) = self.u.class_symbol(class) else { return Prefix::Type(p) };
        let module = match self.u.is_java_statics(c) {
            true => self.u.source_module(c),
            false if self.u.symbol(s).flags.has(STATIC) && !self.u.is_module_class(c) => {
                self.u.companion_module(c)
            }
            false => return Prefix::Type(p),
        };
        match module {
            Some(m) => Prefix::Type(type_ref(self.u, m)),
            None => Prefix::JavaStatics(c),
        }
    }

    /// The modifier of `sym`'s access qualifier, if any, and `flags` without the ones it
    /// replaces (the `privateWithin` part of dotty's `pickleModifiers`).
    pub fn access_qualifier(&mut self, sym: SymbolId, flags: Flags) -> Result<Flags, Refusal> {
        let Some(q) = self.u.access_qualifier(sym) else { return Ok(flags) };
        self.byte(if flags.has(PROTECTED) { PROTECTEDqualified } else { PRIVATEqualified });
        if self.u.symbol(q).kind == Kind::Package {
            self.byte(TYPEREFpkg);
            self.package_name(q);
        } else {
            let class = type_ref(self.u, q);
            self.ty(class, false)?;
        }
        Ok(flags.without(PROTECTED).without(PRIVATE))
    }

    /// Records the parameters of method `m` of clauses `clauses` by the binders of its type.
    pub fn method_params(&mut self, m: SymbolId, clauses: &[Vec<TNodeId>]) {
        let mut t = self.u.info(m);
        for clause in clauses {
            let (Type::Poly { binder, result, .. } | Type::Method { binder, result, .. }) =
                *self.u.types().get(t)
            else {
                return;
            };
            self.method_binders.insert(binder, clause.iter().filter_map(|&p| self.tpd.sym(p)).collect());
            t = result;
        }
    }

    /// A reference without a prefix to a definition of the quote, by its address.
    pub fn direct_ref(&mut self, s: SymbolId) -> Result<(), Refusal> {
        self.byte(if self.u.symbol(s).kind.is_type() { TYPEREFdirect } else { TERMREFdirect });
        self.sym_addr(s);
        Ok(())
    }

    fn prefix(&mut self, prefix: Prefix) -> Result<(), Refusal> {
        match prefix {
            Prefix::Type(t) => self.ty(t, false),
            Prefix::PackageThis(p) => {
                self.package_this(p);
                Ok(())
            }
            Prefix::JavaStatics(c) => {
                let owner = self.u.symbol(c).owner.ok_or("a Java class without owner")?;
                self.byte(TERMREF);
                self.name(&Name::utf8(&self.u.symbol(c).name.clone()));
                self.prefix(Prefix::PackageThis(owner))
            }
        }
    }

    /// `prefix.name`, through the symbol it resolves to if it does.
    pub fn named_ref(&mut self, prefix: TypeId, name: &str, is_type: bool) -> Result<(), Refusal> {
        if let Some(s) = self.u.resolve(prefix, name, is_type) {
            return self.sym_ref(Some(prefix), s);
        }
        let info = (!is_type).then(|| self.u.term_refinement_named(prefix, name));
        if let Some(info) = info
            && info.is_none_or(|i| matches!(self.u.types().get(i), Type::Method { .. } | Type::Poly { .. }))
        {
            return Err("an unresolved term reference, its signature not known");
        }
        self.byte(if is_type { TYPEREF } else { TERMREF });
        self.name(&Name::utf8(name));
        self.ty(prefix, false)
    }
}
