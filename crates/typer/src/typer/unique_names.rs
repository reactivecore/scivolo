//! The `$N$` names of desugaring and typing numbered in the order the typer meets them, as
//! dotty's desugaring takes them from the counter of `UniqueName` while it types.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::names::Name;
use tasty::SymbolId;

/// The `$N$` names met, in order, and the symbols defined with them.
#[derive(Clone, Default)]
pub struct UniqueOrder {
    met: Vec<Name>,
    seen: FxHashSet<Name>,
    syms: Vec<SymbolId>,
}

impl Typer<'_> {
    /// Records `sym`, defined with `name`, if `name` is a `$N$` name; the names desugared with
    /// it count as met with it.
    pub fn defined_unique(&mut self, name: Name, sym: SymbolId) {
        if !self.fresh.is_unique(name) {
            return;
        }
        for n in self.fresh.group(name) {
            if self.syms.uniques.seen.insert(n) {
                self.syms.uniques.met.push(n);
            }
        }
        self.syms.uniques.syms.push(sym);
    }

    /// Renames the symbols defined with `$N$` names, and the trees that define or refer to them,
    /// to `$1$`, `$2$`, … in the order their names were met.
    pub fn renumber_uniques(&mut self) {
        let order = std::mem::take(&mut self.syms.uniques);
        let mut renamed: FxHashMap<Name, Name> = FxHashMap::default();
        for (i, &n) in order.met.iter().enumerate() {
            let new = self.name(&format!("${}$", i + 1));
            if new != n {
                renamed.insert(n, new);
            }
        }
        let mut syms: FxHashMap<SymbolId, Name> = FxHashMap::default();
        for s in order.syms {
            let old = self.name(&self.u.symbol(s).name.clone());
            if let Some(&new) = renamed.get(&old) {
                syms.insert(s, new);
            }
        }
        for (&s, &new) in &syms {
            let text = self.text(new).to_owned();
            self.u.rename(s, &text);
        }
        if syms.is_empty() {
            return;
        }
        for i in 0..self.tpd.len() {
            let t = TNodeId::new(i);
            let Some(&new) = self.tpd.sym(t).and_then(|s| syms.get(&s)) else { continue };
            if let T::Ident { name }
            | T::Select { name, .. }
            | T::Bind { name, .. }
            | T::ValDef { name, .. } = &mut self.tpd.get_mut(t).tree
            {
                name.name = new;
            }
        }
    }
}
