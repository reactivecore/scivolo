// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/InlineReducer.scala
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! The values a reduction binds, erased when they refer to erased definitions and dropped when
//! unusable (dotty's `newSym`, `adjustErased` and the `cleanupUnusable` of
//! `reduceInlineMatch`).

use super::Reducer;
use crate::inlining::Unsupported;
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::{ERASED, SYNTHETIC};
use tasty::{Flags, Kind, Symbol, SymbolId, TypeId};

impl Reducer<'_, '_> {
    /// A new synthetic value named `prefix` and a fresh number, of type `info`, owned by the
    /// call's owner.
    pub fn new_val(&mut self, prefix: &str, info: TypeId) -> SymbolId {
        let name = self.i.fresh.next(self.i.names, prefix);
        let s = self.i.u.define(Symbol {
            name: self.i.names.text(name).to_owned(),
            kind: Kind::Val,
            owner: Some(self.owner),
            flags: Flags::default().with(SYNTHETIC),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.i.u.set_info(s, info);
        s
    }

    /// `bindings` without those of unusable values, and `rhs`; `Err` if a reference to one
    /// remains.
    pub fn without_unusable(
        &mut self,
        bindings: Vec<TNodeId>,
        rhs: TNodeId,
    ) -> Result<(Vec<TNodeId>, TNodeId), Unsupported> {
        let kept: Vec<TNodeId> = bindings
            .into_iter()
            .filter(|&b| self.i.tpd.sym(b).is_none_or(|s| !self.unusable.contains(&s)))
            .collect();
        for &t in kept.iter().chain([&rhs]) {
            if let Some(r) = self.unusable_ref_in(t) {
                return Err((self.i.tpd.span(r), "reference to an erased scrutinee of an inline match"));
            }
        }
        Ok((kept, rhs))
    }

    fn unusable_ref_in(&self, t: TNodeId) -> Option<TNodeId> {
        if matches!(self.i.tpd.tree(t), T::Ident { .. })
            && self.i.tpd.sym(t).is_some_and(|s| self.unusable.contains(&s))
        {
            return Some(t);
        }
        self.i.tpd.children(t).into_iter().find_map(|c| self.unusable_ref_in(c))
    }

    /// Flags `s` erased.
    pub fn erase(&mut self, s: SymbolId) {
        let flags = self.i.u.symbol(s).flags.with(ERASED);
        self.i.u.set_flags(s, flags);
    }

    /// Flags `s`, defined by `rhs`, erased if `rhs` refers to an erased definition, and unusable
    /// if to an unusable one.
    pub fn adjust_erased(&mut self, s: SymbolId, rhs: TNodeId) {
        if let T::Ident { .. } = self.i.tpd.tree(rhs)
            && let Some(r) = self.i.tpd.sym(rhs)
            && self.i.is_erased(r)
        {
            self.erase(s);
            if self.unusable.contains(&r) {
                self.unusable.insert(s);
            }
        }
        for c in self.i.tpd.children(rhs) {
            self.adjust_erased(s, c);
        }
    }

    /// Whether `t` refers to an erased definition.
    pub fn refers_to_erased(&self, t: TNodeId) -> bool {
        self.i.tpd.sym(t).is_some_and(|s| self.i.is_erased(s))
            || self.i.tpd.children(t).into_iter().any(|c| self.refers_to_erased(c))
    }
}
