// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The `$N$` names of desugaring and typing numbered per file in the order the typer meets them,
//! as dotty's desugaring takes them from the counter of `UniqueName` while it types.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::{NodeId, Span};
use syntax::names::Name;
use tasty::{Kind, SymbolId, Type, TypeId};

/// The `$N$` names met, in order, with their files, and the symbols defined with them.
#[derive(Clone, Default)]
pub struct UniqueOrder {
    met: Vec<(usize, Name)>,
    seen: FxHashSet<(usize, Name)>,
    syms: Vec<(SymbolId, usize)>,
}

impl Typer<'_> {
    /// Makes `prefix` followed by the next number for it in the file of `span`.
    pub fn fresh_name(&mut self, span: Span, prefix: &str) -> Name {
        let file = self.file_of(span);
        self.fresh.next_in(file, self.names, prefix)
    }

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

    /// Completes the member classes of `cls` in order if its given self type has them as members,
    /// as dotty's `classExistsOnSelf` does by looking each up in the self type.
    pub fn complete_classes_on_self(&mut self, cls: SymbolId) {
        let Some(self_type) = self.u.given_self_type(cls) else { return };
        if self.u.class_symbol(self_type) == Some(cls) || !self.has_members_of(self_type, cls) {
            return;
        }
        for d in self.u.decls(cls).unwrap_or_default() {
            if self.u.symbol(d).kind == Kind::Class {
                self.complete(d);
            }
        }
    }

    /// Whether type `t` derives from class `cls`, or one part of it if an intersection.
    fn has_members_of(&mut self, t: TypeId, cls: SymbolId) -> bool {
        let t = self.u.widen(t);
        if let Some(s) = self.u.sym_of(t) {
            self.complete(s);
        }
        let dealiased = self.u.dealias_step(t);
        if dealiased != t {
            return self.has_members_of(dealiased, cls);
        }
        if let Type::And(a, b) = *self.u.ty(t) {
            return self.has_members_of(a, cls) || self.has_members_of(b, cls);
        }
        let Some(own) = self.u.class_symbol(t) else { return false };
        self.complete(own);
        self.u.derives_from(own, cls)
    }

    /// Meets `names`, made by the typer at `span`.
    pub fn meet_uniques(&mut self, span: Span, names: &[Name]) {
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
