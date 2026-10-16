//! A class and an object of the same name in different source files of one unit.

use crate::typer::Typer;
use syntax::ast::NodeId;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Refuses a package-level object whose class namesake is defined in another source file of
    /// the unit.
    // Temporary until #115 (companions across files): `Universe` links companions by name, not
    // by file, so such a pair would count as companions.
    pub fn check_split_companions(&mut self) {
        if self.file_starts.is_empty() {
            return;
        }
        let mut defs: Vec<(SymbolId, NodeId)> = self.syms.def_trees.iter().map(|(&s, &t)| (s, t)).collect();
        defs.sort_by_key(|&(_, t)| self.ast.span(t).start());
        for (module, tree) in defs {
            let Some(owner) = self.u.symbol(module).owner else { continue };
            if !self.u.is_module(module) || self.u.symbol(owner).kind != Kind::Package {
                continue;
            }
            let name = self.u.symbol(module).name.clone();
            let decls = self.u.decl(owner, &name).unwrap_or_default();
            let class = decls.into_iter().find(|&c| self.u.is_class(c) && !self.u.is_module_class(c));
            let Some(&class_tree) = class.and_then(|c| self.syms.def_trees.get(&c)) else { continue };
            if self.file_of(self.ast.span(class_tree)) != self.file_of(self.ast.span(tree)) {
                self.unsupported(tree, "a class and its companion object in different source files");
            }
        }
    }
}
