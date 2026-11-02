// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! References to symbols and named members (dotty's `NamedType` case of `pickleNewType`).

use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use tasty::tags::*;
use tasty::write::Name;
use tasty::{Kind, SymbolId, Type, TypeId};

/// The prefix of a reference: a type, or the `this` of a package.
#[derive(Clone, Copy)]
enum Prefix {
    Type(TypeId),
    PackageThis(SymbolId),
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
        let prefix = match (prefix, owner.map(|o| (o, self.u.symbol(o).kind))) {
            (Some(p), _) => Prefix::Type(p),
            (None, _) if self.is_defined_here(s) => return self.direct_ref(s),
            (None, Some((o, Kind::Package))) => Prefix::PackageThis(o),
            (None, Some((o, Kind::Class))) => {
                let class = type_ref(self.u, o);
                Prefix::Type(self.u.intern_type(Type::This(class)))
            }
            _ => return self.direct_ref(s),
        };
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

    /// A reference without a prefix to a definition of the quote, by its address.
    fn direct_ref(&mut self, s: SymbolId) -> Result<(), Refusal> {
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
        }
    }

    /// `prefix.name`, through the symbol it resolves to if it does.
    pub fn named_ref(&mut self, prefix: TypeId, name: &str, is_type: bool) -> Result<(), Refusal> {
        if let Some(s) = self.u.resolve(prefix, name, is_type) {
            return self.sym_ref(Some(prefix), s);
        }
        if !is_type {
            return Err("an unresolved term reference, its signature not known");
        }
        self.byte(if is_type { TYPEREF } else { TERMREF });
        self.name(&Name::utf8(name));
        self.ty(prefix, false)
    }
}
