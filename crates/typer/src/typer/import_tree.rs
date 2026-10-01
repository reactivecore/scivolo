//! The typed tree of an import, and the checks on its selectors (dotty's `typedImport`,
//! `checkImportSelectors`).

use super::Typer;
use super::imports::{ImportEntry, ImportInfo};
use crate::ctx::Level;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, TName, Tree};

impl Typer<'_> {
    /// The typed import `id`, its path typed when its level was first searched or now.
    pub fn typed_import(&mut self, id: NodeId, selectors: List) -> TNodeId {
        let level = self.import_levels[&id];
        let Level::Import(i) = *self.levels.get(level) else { unreachable!() };
        self.import_info(i);
        let ImportEntry::Done(ImportInfo { expr: Some(expr), .. }) = &self.imports[i] else {
            return self.unsupported(id, "cyclic import");
        };
        let expr = *expr;
        if let Err(what) = self.check_selectors(i) {
            return self.unsupported(id, what);
        }
        let items = self.ast.items(selectors).to_vec();
        let typed: Vec<TNodeId> = items.into_iter().map(|s| self.untyped_selector(s)).collect();
        let selectors = self.tpd.list(&typed);
        let span = self.ast.span(id);
        self.tpd.node(T::Import { expr, selectors }, span, None, None)
    }

    /// Checks that the named selectors of import `i` name members of its path, that no name is
    /// imported twice, and that no name is both imported and unimported.
    fn check_selectors(&mut self, i: usize) -> Result<(), &'static str> {
        let ImportEntry::Done(info) = &self.imports[i] else { return Ok(()) };
        let qual = info.qual;
        let named: Vec<(String, String)> = info
            .selectors
            .iter()
            .filter(|s| s.name != "_" && !s.name.is_empty())
            .map(|s| (s.name.clone(), s.rename.clone()))
            .collect();
        for (k, (name, rename)) in named.iter().enumerate() {
            if self.type_members(qual, name).is_empty() {
                return Err("import of a missing member");
            }
            let earlier = &named[..k];
            if rename != "_" && earlier.iter().any(|(_, r)| r == rename) {
                return Err("name imported twice");
            }
            let imported = |r: &str| r != "_";
            if earlier.iter().any(|(n, r)| n == name && imported(r) != imported(rename)) {
                return Err("name imported and unimported");
            }
        }
        Ok(())
    }

    /// An import selector as dotty keeps it: untyped identifiers.
    fn untyped_selector(&mut self, s: NodeId) -> TNodeId {
        let Tree::ImportSelector { imported, renamed, bound } = *self.ast.tree(s) else {
            return self.unsupported(s, "import selector");
        };
        if let Some(b) = bound {
            return self.unsupported(b, "import bound");
        }
        let ident = |t: &mut Self, id: NodeId| match *t.ast.tree(id) {
            Tree::Ident { name } => {
                let span = t.ast.span(id);
                t.tpd.node(T::Ident { name: TName { name: name.name, is_type: false } }, span, None, None)
            }
            _ => t.unsupported(id, "import selector"),
        };
        let imported = ident(self, imported);
        let renamed = renamed.map(|r| ident(self, r));
        let span = self.ast.span(s);
        self.tpd.node(T::ImportSelector { imported, renamed, bound: None }, span, None, None)
    }
}
