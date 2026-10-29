// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! Objects, their classes and companions: the class of object `C` is `C$`, declared beside it
//! (dotty's `moduleClass`, `sourceModule`, `companionClass`, `companionModule`).

use super::{SymbolId, Universe};
use crate::Type;

/// The name of the class of object `module`: `C$` for `C`.
pub fn module_class_name(module: &str) -> String {
    format!("{module}$")
}

/// The name of the object whose class is named `class`: `C` for `C$`.
pub fn module_name(class: &str) -> Option<&str> {
    class.strip_suffix('$')
}

impl Universe {
    /// The class of object `module`: the one declared beside it, else, or among several of its
    /// name, the class of its type.
    pub fn module_class(&mut self, module: SymbolId) -> Option<SymbolId> {
        let owner = self.symbol(module).owner?;
        let name = module_class_name(&self.symbol(module).name);
        let beside: Vec<SymbolId> =
            self.decl(owner, &name).unwrap_or_default().into_iter().filter(|&c| self.is_class(c)).collect();
        if let [c] = beside[..] {
            return Some(c);
        }
        let info = self.info(module);
        match *self.types().get(info) {
            Type::Sym(_, c) if self.is_class(c) => Some(c),
            _ => beside.first().copied(),
        }
    }

    /// The class of object `module` declared in `owner`.
    pub fn module_class_in(&mut self, owner: SymbolId, module: &str) -> Option<SymbolId> {
        let name = module_class_name(module);
        self.decl(owner, &name).ok()?.into_iter().find(|&c| self.is_class(c))
    }

    /// The object of module class `c`: among several of its name, the one typed `c`.
    pub fn source_module(&mut self, c: SymbolId) -> Option<SymbolId> {
        let owner = self.symbol(c).owner?;
        let name = module_name(&self.symbol(c).name)?.to_owned();
        let found: Vec<SymbolId> =
            self.decl(owner, &name).ok()?.into_iter().filter(|&s| self.is_module(s)).collect();
        let typed_c = |u: &mut Self, s: SymbolId| {
            let info = u.info(s);
            matches!(*u.types().get(info), Type::Sym(_, x) if x == c)
        };
        match found[..] {
            [s] => Some(s),
            _ => found.iter().copied().find(|&s| typed_c(self, s)).or(found.first().copied()),
        }
    }

    /// The class whose companion object has module class `c`.
    pub fn companion_class(&mut self, c: SymbolId) -> Option<SymbolId> {
        let owner = self.symbol(c).owner?;
        let name = module_name(&self.symbol(c).name)?.to_owned();
        self.decl(owner, &name).ok()?.into_iter().find(|&s| self.is_class(s) && !self.is_module_class(s))
    }

    /// The companion object of class `c`, also one renamed while its class keeps `c`'s name.
    pub fn companion_module(&mut self, c: SymbolId) -> Option<SymbolId> {
        let owner = self.symbol(c).owner?;
        let name = self.symbol(c).name.clone();
        let named = self.decl(owner, &name).unwrap_or_default().into_iter().find(|&s| self.is_module(s));
        if named.is_some() {
            return named;
        }
        let class = self.module_class_in(owner, &name)?;
        let decls = self.decls(owner).unwrap_or_default();
        decls.into_iter().find(|&s| self.is_module(s) && self.module_class(s) == Some(class))
    }
}
