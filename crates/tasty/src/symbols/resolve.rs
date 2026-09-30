//! Resolving named references (`prefix.name`) to symbols: members of packages, and declared or
//! inherited members of classes and objects.

use super::{Kind, SymbolId, Universe};
use crate::types::{Type, TypeId};

impl Universe {
    /// The symbol `prefix.name` refers to, a type symbol if `ty`, else a term symbol.
    pub fn resolve(&mut self, prefix: TypeId, name: &str, ty: bool) -> Option<SymbolId> {
        let owner = self.prefix_owner(prefix)?;
        let found = match self.symbol(owner).kind {
            Kind::Class => self.member(owner, name),
            _ => self.decl(owner, name).ok()?,
        };
        let is_type =
            |u: &Self, s: SymbolId| matches!(u.symbol(s).kind, Kind::Class | Kind::Type | Kind::TypeParam);
        found.into_iter().find(|&s| is_type(self, s) == ty)
    }

    /// The package or class whose members a prefix selects.
    fn prefix_owner(&mut self, prefix: TypeId) -> Option<SymbolId> {
        match self.types.get(prefix).clone() {
            Type::Package(p) => Some(p),
            Type::This(c) => self.prefix_owner(c),
            Type::Sym(_, s) if matches!(self.symbol(s).kind, Kind::Class | Kind::Package) => Some(s),
            Type::Sym(_, s) if self.symbol(s).kind == Kind::Val => self.module_class(s),
            Type::TermRef(p, n) => match self.resolve(p, &n, false)? {
                pkg if self.symbol(pkg).kind == Kind::Package => Some(pkg),
                term => self.module_class(term),
            },
            Type::TypeRef(p, n) => self.resolve(p, &n, true),
            _ => None,
        }
    }

    /// The class of object `module`: the class `name$` declared beside it.
    pub fn module_class(&mut self, module: SymbolId) -> Option<SymbolId> {
        let (owner, name) = (self.symbol(module).owner?, format!("{}$", self.symbol(module).name));
        self.decl(owner, &name).ok()?.into_iter().find(|&c| self.symbol(c).kind == Kind::Class)
    }
}
