//! Objects, their classes and companions: the class of object `C` is `C$`, declared beside it
//! (dotty's `moduleClass`, `sourceModule`, `companionClass`, `companionModule`).

use super::{SymbolId, Universe};

/// The name of the class of object `module`: `C$` for `C`.
pub fn module_class_name(module: &str) -> String {
    format!("{module}$")
}

/// The name of the object whose class is named `class`: `C` for `C$`.
pub fn module_name(class: &str) -> Option<&str> {
    class.strip_suffix('$')
}

impl Universe {
    /// The class of object `module`.
    pub fn module_class(&mut self, module: SymbolId) -> Option<SymbolId> {
        let owner = self.symbol(module).owner?;
        let name = self.symbol(module).name.clone();
        self.module_class_in(owner, &name)
    }

    /// The class of object `module` declared in `owner`.
    pub fn module_class_in(&mut self, owner: SymbolId, module: &str) -> Option<SymbolId> {
        let name = module_class_name(module);
        self.decl(owner, &name).ok()?.into_iter().find(|&c| self.is_class(c))
    }

    /// The object of module class `c`.
    pub fn source_module(&mut self, c: SymbolId) -> Option<SymbolId> {
        let owner = self.symbol(c).owner?;
        let name = module_name(&self.symbol(c).name)?.to_owned();
        self.decl(owner, &name).ok()?.into_iter().find(|&s| self.is_module(s))
    }

    /// The class whose companion object has module class `c`.
    pub fn companion_class(&mut self, c: SymbolId) -> Option<SymbolId> {
        let owner = self.symbol(c).owner?;
        let name = module_name(&self.symbol(c).name)?.to_owned();
        self.decl(owner, &name).ok()?.into_iter().find(|&s| self.is_class(s) && !self.is_module_class(s))
    }

    /// The companion object of class `c`.
    pub fn companion_module(&mut self, c: SymbolId) -> Option<SymbolId> {
        let owner = self.symbol(c).owner?;
        let name = self.symbol(c).name.clone();
        self.decl(owner, &name).ok()?.into_iter().find(|&s| self.is_module(s))
    }
}
