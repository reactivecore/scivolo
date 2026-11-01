// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The imports and import selectors `quotes.reflect` makes.

use crate::host::Host;
use syntax::ast::TName;
use typer::tpd::{TNodeId, Tree as T};

impl Host {
    /// The selector of the name before the first newline of `names`, renamed to the name after it
    /// if there is one, and of bound `bound`; a `given` selector imports the empty name.
    pub fn import_selector(&mut self, bound: Option<TNodeId>, names: String) -> TNodeId {
        let (imported, renamed) = match names.split_once('\n') {
            Some((imported, renamed)) => (imported, Some(renamed)),
            None => (names.as_str(), None),
        };
        let imported = self.untyped_ident(imported);
        let renamed = renamed.map(|r| self.untyped_ident(r));
        self.tpd.node(T::ImportSelector { imported, renamed, bound }, self.expansion, None, None)
    }

    /// `Import.apply` of `expr` and `selectors`.
    pub fn import(&mut self, expr: TNodeId, selectors: Vec<TNodeId>) -> TNodeId {
        let selectors = self.tpd.list(&selectors);
        self.tpd.node(T::Import { expr, selectors, export: false }, self.expansion, None, None)
    }

    fn untyped_ident(&mut self, name: &str) -> TNodeId {
        let name = TName { name: self.names.intern(name), is_type: false };
        self.tpd.node(T::Ident { name }, self.expansion, None, None)
    }
}
