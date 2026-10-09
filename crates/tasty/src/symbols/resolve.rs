//! Resolving named references (`prefix.name`) to symbols: members of packages, and declared or
//! inherited members of classes and objects.

use super::{Kind, SymbolId, Universe};
use crate::tags::OBJECT;
use crate::types::{Type, TypeId};

/// How many resolutions may be in progress at once, one inside another.
const MAX_RESOLVING: u32 = 64;

impl Universe {
    /// The symbol `prefix.name` refers to, a type symbol if `ty`, else a term symbol; `None` for
    /// a reference whose prefix leads back to itself deeper than `MAX_RESOLVING`.
    pub fn resolve(&mut self, prefix: TypeId, name: &str, ty: bool) -> Option<SymbolId> {
        if self.resolving >= MAX_RESOLVING {
            return None;
        }
        self.resolving += 1;
        let found = self.resolve_in(prefix, name, ty);
        self.resolving -= 1;
        found
    }

    fn resolve_in(&mut self, prefix: TypeId, name: &str, ty: bool) -> Option<SymbolId> {
        let owner = self.prefix_owner(prefix)?;
        let wanted = move |u: &Self, s: SymbolId| {
            matches!(u.symbol(s).kind, Kind::Class | Kind::Type | Kind::TypeParam) == ty
        };
        let found = match self.symbol(owner).kind {
            Kind::Class => self.member_where(owner, name, wanted),
            _ => self.decl(owner, name).ok()?,
        };
        found.into_iter().find(|&s| wanted(self, s))
    }

    /// The package or class whose members a prefix selects.
    fn prefix_owner(&mut self, prefix: TypeId) -> Option<SymbolId> {
        match self.types.get(prefix).clone() {
            Type::Package(p) => Some(p),
            Type::This(c) => self.prefix_owner(c),
            Type::Sym(_, s) => self.symbol_owner(s),
            Type::TermRef(p, n) => {
                let term = self.resolve(p, &n, false)?;
                self.symbol_owner(term)
            }
            _ => self.class_symbol(prefix),
        }
    }

    /// The package or class whose members a path to `s` selects: `s` itself, an object's class,
    /// the class of a value's type.
    fn symbol_owner(&mut self, s: SymbolId) -> Option<SymbolId> {
        match self.symbol(s).kind {
            Kind::Class | Kind::Package => Some(s),
            Kind::Val if self.symbol(s).flags.has(OBJECT) => self.module_class(s),
            Kind::Val | Kind::Param | Kind::Def => {
                let info = self.info(s);
                self.prefix_owner(info)
            }
            Kind::Type | Kind::TypeParam => {
                let t = self.intern_type(Type::Sym(None, s));
                self.class_symbol(t)
            }
        }
    }

    /// The class of object `module`: the class `name$` declared beside it, else the class of its
    /// type.
    pub fn module_class(&mut self, module: SymbolId) -> Option<SymbolId> {
        let (owner, name) = (self.symbol(module).owner?, format!("{}$", self.symbol(module).name));
        let beside = self.decl(owner, &name).ok().into_iter().flatten();
        let class = beside.into_iter().find(|&c| self.symbol(c).kind == Kind::Class);
        if class.is_some() {
            return class;
        }
        let info = self.info(module);
        match *self.types().get(info) {
            Type::Sym(_, c) if self.symbol(c).kind == Kind::Class => Some(c),
            _ => None,
        }
    }
}
