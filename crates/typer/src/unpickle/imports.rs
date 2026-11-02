// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
//! Import statements (dotty's `readImportOrExport`, `readSelectors`).

use super::{Refusal, Unpickler};
use crate::tpd::{TNodeId, Tree as T};
use tasty::NodeId;
use tasty::tags::*;

impl Unpickler<'_> {
    /// `IMPORT`: its path and its selectors, each an imported name with its rename or the bound
    /// of a `given` selector.
    pub fn import(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        let expr = self.term(*k.first().ok_or("import without a path")?)?;
        let mut selectors = Vec::new();
        let mut rest = k[1..].iter().copied().peekable();
        while let Some(s) = rest.next() {
            if self.tag(s) != IMPORTED {
                return Err("import selector of another kind in a library inline body");
            }
            let imported = self.selector_ident(s, s);
            let renamed = rest.next_if(|&r| self.tag(r) == RENAMED).map(|r| self.selector_ident(r, s));
            let bound = match rest.next_if(|&b| self.tag(b) == BOUNDED) {
                Some(b) => Some(self.tpt(*self.kids(b).first().ok_or("import bound without a type")?)?),
                None => None,
            };
            let t = self.bare(T::ImportSelector { imported, renamed, bound }, None, None);
            self.tpd.get_mut(t).span = self.tpd.span(imported).to_synthetic();
            selectors.push(t);
        }
        let selectors = self.tpd.list(&selectors);
        let t = self.bare(T::Import { expr, selectors, export: false }, None, None);
        Ok(self.positioned(id, t))
    }

    /// The untyped identifier of the name `IMPORTED` or `RENAMED` at `id` holds, in the span of
    /// selector `at`.
    fn selector_ident(&mut self, id: NodeId, at: NodeId) -> TNodeId {
        let name = self.tname(id, false);
        let t = self.bare(T::Ident { name }, None, None);
        self.positioned(at, t)
    }
}
