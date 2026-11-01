//! The `$N$` names of desugaring and typing numbered per file in the order the typer meets them,
//! as dotty's desugaring takes them from the counter of `UniqueName` while it types.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::{NodeId, Span};
use syntax::names::Name;
use tasty::SymbolId;

/// The `$N$` names met, in order, with their files, and the symbols defined with them.
#[derive(Clone, Default)]
pub struct UniqueOrder {
    met: Vec<(usize, Name)>,
    seen: FxHashSet<(usize, Name)>,
    syms: Vec<(SymbolId, usize)>,
}

impl Typer<'_> {
    /// Meets the names of the pattern definition the namer enters at `stat`, if any.
    pub fn enter_uniques(&mut self, stat: NodeId) {
        let span = self.ast.span(stat);
        if let Some(group) = self.fresh.uniques.on_entry(span.start()) {
            let group = group.to_vec();
            self.meet_uniques(span, &group);
        }
    }

    /// Meets the names of the `for` the typer types at `id`, if any.
    pub fn type_uniques(&mut self, id: NodeId) {
        let span = self.ast.span(id);
        if let Some(group) = self.fresh.uniques.on_typing(span.start()) {
            let group = group.to_vec();
            self.meet_uniques(span, &group);
        }
    }

    /// Records `sym`, defined at `span` with `name`, if `name` is a `$N$` name.
    pub fn defined_unique(&mut self, name: Name, sym: SymbolId, span: Span) {
        if self.fresh.uniques.contains(name) {
            self.meet_uniques(span, &[name]);
            let file = self.file_of(span);
            self.syms.uniques.syms.push((sym, file));
        }
    }

    fn meet_uniques(&mut self, span: Span, names: &[Name]) {
        let file = self.file_of(span);
        let order = &mut self.syms.uniques;
        for &n in names {
            if order.seen.insert((file, n)) {
                order.met.push((file, n));
            }
        }
    }

    /// Renames the symbols defined with `$N$` names, and the trees that define or refer to them,
    /// to `$1$`, `$2$`, … of their file in the order their names were met.
    pub fn renumber_uniques(&mut self) {
        let order = std::mem::take(&mut self.syms.uniques);
        let mut counts: FxHashMap<usize, usize> = FxHashMap::default();
        let mut renamed: FxHashMap<(usize, Name), Name> = FxHashMap::default();
        for &(file, n) in &order.met {
            let count = counts.entry(file).or_default();
            *count += 1;
            let new = self.name(&format!("${count}$"));
            if new != n {
                renamed.insert((file, n), new);
            }
        }
        let mut syms: FxHashMap<SymbolId, Name> = FxHashMap::default();
        for (s, file) in order.syms {
            let old = self.name(&self.u.symbol(s).name.clone());
            if let Some(&new) = renamed.get(&(file, old)) {
                syms.insert(s, new);
            }
        }
        if syms.is_empty() {
            return;
        }
        for (&s, &new) in &syms {
            let text = self.text(new).to_owned();
            self.u.rename(s, &text);
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
