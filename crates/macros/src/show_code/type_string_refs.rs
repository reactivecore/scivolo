// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/core/Symbols.scala
//! References to symbols and packages as `TypeRepr.toString` shows them.

use super::type_string::{REFUSED, Shown, Text};
use names::nme;
use tasty::{Kind, SymbolId, Type, TypeId};

/// The reference to package `name` owned by package `owner`, as its `TermRef` (dotty's
/// `termRef`) or as its class's `this`.
fn package_ref(owner: &str, name: &str, this: bool) -> String {
    let owner_this = format!("ThisType(TypeRef(NoPrefix,module class {owner}))");
    match this {
        true => format!("ThisType(TypeRef({owner_this},module class {name}))"),
        false => format!("TermRef({owner_this},object {name})"),
    }
}

/// The reference to the package of qualified name `name`, as `package_ref`.
pub fn package_named(name: &str, this: bool) -> String {
    match name.rsplit_once('.') {
        Some((owner, last)) => package_ref(owner.rsplit('.').next().unwrap_or(owner), last, this),
        None => package_ref(nme::ROOT, name, this),
    }
}

impl Text<'_> {
    /// Package `p` as `package_ref` shows it; the root as the empty package's `this`.
    pub fn package(&mut self, p: SymbolId, this: bool) -> Shown {
        let root = self.h.u.root();
        if p == root {
            self.out.push_str(&package_ref(nme::ROOT, nme::EMPTY_PACKAGE, true));
            return Ok(());
        }
        let owner = self.h.u.symbol(p).owner.ok_or(REFUSED)?;
        let owner = if owner == root { nme::ROOT } else { &self.h.u.symbol(owner).name };
        let text = package_ref(owner, &self.h.u.symbol(p).name, this);
        self.out.push_str(&text);
        Ok(())
    }

    /// The `this` of class or package `c`.
    pub fn this(&mut self, c: TypeId) -> Shown {
        match *self.h.u.ty(c) {
            Type::Package(p) => self.package(p, true),
            Type::Sym(_, p) if self.h.u.symbol(p).kind == Kind::Package => self.package(p, true),
            Type::PackageName(n) => {
                self.out.push_str(&package_named(self.h.u.name(n), true));
                Ok(())
            }
            _ => self.node("ThisType", &[c], ""),
        }
    }

    /// A reference to `s` (dotty's `designatorFor`): by name if its prefix is its owner's `this`,
    /// else by symbol; without a prefix through its owner's `this` (dotty's `namedType`).
    pub fn sym_ref(&mut self, prefix: Option<TypeId>, s: SymbolId) -> Shown {
        let sym = self.h.u.symbol(s);
        let (kind, name, owner) = (sym.kind, sym.name.clone(), sym.owner);
        if kind == Kind::Package {
            return self.package(s, false);
        }
        let what = if kind.is_term() { "TermRef" } else { "TypeRef" };
        let symbol = format!("{} {name}", self.kind_string(s));
        let Some(prefix) = prefix else {
            self.out.push_str(&format!("{what}("));
            match owner {
                Some(o) if self.h.u.symbol(o).kind == Kind::Package => {
                    let name =
                        if o == self.h.u.root() { nme::EMPTY_PACKAGE } else { &self.h.u.symbol(o).name };
                    let text = format!("ThisType(TypeRef(NoPrefix,module class {name}))");
                    self.out.push_str(&text);
                }
                Some(o) if self.h.u.is_class(o) => {
                    self.out.push_str("ThisType(");
                    self.sym_ref(None, o)?;
                    self.out.push(')');
                }
                _ => self.out.push_str("NoPrefix"),
            }
            self.out.push_str(&format!(",{symbol})"));
            return Ok(());
        };
        let owner_this = match *self.h.u.ty(prefix) {
            Type::This(c) => match *self.h.u.ty(c) {
                Type::Package(p) => Some(p),
                _ => self.h.u.class_symbol(c),
            },
            _ => None,
        };
        let designator = if owner_this.is_some() && owner_this == owner { name } else { symbol };
        self.node(what, &[prefix], &format!(",{designator}"))
    }

    /// The kind of `s` as `SymDenotation.toString` names it.
    fn kind_string(&self, s: SymbolId) -> &'static str {
        let u = &self.h.u;
        match u.symbol(s).kind {
            _ if u.is_module_class(s) => "module class",
            _ if u.is_trait(s) => "trait",
            Kind::Class => "class",
            Kind::Type | Kind::TypeParam => "type",
            _ if u.is_module(s) => "object",
            Kind::Package => "object",
            Kind::Def => "method",
            _ => "val",
        }
    }
}
