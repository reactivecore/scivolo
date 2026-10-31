// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/Extractors.scala
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Imports and exports as `Printer.TreeCode` and `Printer.TreeStructure` show them.

use super::structure::STRUCTURE;
use super::{Code, REFUSED, Shown};
use typer::tpd::{TNodeId, Tree as T};

enum Selector {
    Simple(String),
    Omit(String),
    Rename(String, String),
    Given(Option<TNodeId>),
}

impl Code<'_> {
    /// `import` or `export` `t` (dotty's `printTree` of an `Import` or `Export`).
    pub fn import(&mut self, t: TNodeId) -> Shown {
        let T::Import { expr, selectors, export } = self.tree_of(t) else { return Err(REFUSED) };
        self.push(if export { "export " } else { "import " });
        self.tree(expr)?;
        self.push(".{");
        for (i, s) in self.items(selectors).into_iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            match self.selector(s).ok_or(REFUSED)? {
                Selector::Simple(n) => self.push(&n),
                Selector::Omit(n) => self.push(&format!("{n} => _")),
                Selector::Rename(n, r) => self.push(&format!("{n} => {r}")),
                Selector::Given(None) => self.push("given"),
                Selector::Given(Some(b)) => {
                    self.push("given ");
                    self.tree(b)?;
                }
            }
        }
        self.push("}");
        Ok(())
    }

    /// `Import(..)` or `Export(..)` of `t` (dotty's `Extractors.visitTree`).
    pub fn import_structure(&mut self, t: TNodeId) -> Shown {
        let T::Import { expr, selectors, export } = self.tree_of(t) else { return Err(STRUCTURE) };
        self.push(if export { "Export(" } else { "Import(" });
        self.structure(expr)?;
        let items = self.items(selectors);
        self.push(if items.is_empty() { ", Nil" } else { ", List(" });
        for (i, &s) in items.iter().enumerate() {
            self.push(if i > 0 { ", " } else { "" });
            match self.selector(s).ok_or(STRUCTURE)? {
                Selector::Simple(n) => self.push(&format!("SimpleSelector({n})")),
                Selector::Omit(n) => self.push(&format!("OmitSelector({n})")),
                Selector::Rename(n, r) => self.push(&format!("RenameSelector({n}, {r})")),
                Selector::Given(b) => {
                    self.push("GivenSelector(");
                    self.option(b)?;
                    self.push(")");
                }
            }
        }
        self.push(if items.is_empty() { ")" } else { "))" });
        Ok(())
    }

    fn selector(&self, s: TNodeId) -> Option<Selector> {
        let T::ImportSelector { imported, renamed, bound } = self.tree_of(s) else { return None };
        let name = |c: &Self, t| match c.tree_of(t) {
            T::Ident { name } => Some(c.name(name)),
            _ => None,
        };
        let imported = name(self, imported)?;
        Some(match renamed.map(|r| name(self, r)) {
            _ if imported.is_empty() => Selector::Given(bound),
            None => Selector::Simple(imported),
            Some(Some(r)) if r == "_" => Selector::Omit(imported),
            Some(Some(r)) => Selector::Rename(imported, r),
            Some(None) => return None,
        })
    }
}
