// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeTypeMap.scala
//   compiler/src/dotty/tools/dotc/core/Symbols.scala
//! Fresh copies of local classes and type aliases in copied trees: the class with its members,
//! its parents and self type mapped (dotty's `mapSymbols` for classes).

use super::Inlining;
use super::copy::TreeTypeMap;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

impl Inlining<'_> {
    /// The fresh symbol of `s` and, for a class, those of its members, recursively, added to
    /// `made`, their types not yet set.
    pub fn declared_fresh_with_members(
        &mut self,
        m: &mut TreeTypeMap,
        s: SymbolId,
        made: &mut Vec<(SymbolId, SymbolId)>,
    ) {
        made.push((s, self.declared_fresh(m, s)));
        if self.u.symbol(s).kind != Kind::Class {
            return;
        }
        m.classes = true;
        for d in self.u.symbol(s).decls.clone() {
            if !m.syms.contains_key(&d) {
                self.declared_fresh_with_members(m, d, made);
            }
        }
    }

    /// Completes fresh symbol `n` of `s`: a class's parents and self type, else its type, mapped
    /// through `m`, and whether it is abstract.
    pub fn complete_fresh(&mut self, m: &TreeTypeMap, s: SymbolId, n: SymbolId) {
        if self.u.symbol(s).kind == Kind::Class {
            let parents = self.u.parents(s).into_iter().map(|p| self.map_type(m, p)).collect();
            self.u.set_parents(n, parents);
            if let Some(t) = self.u.given_self_type(s) {
                let t = self.map_type(m, t);
                self.u.set_self_type(n, t);
            }
            if let Some(kind) = self.tpd.anon_mirror(s) {
                self.tpd.mark_anon_mirror(n, kind);
            }
        } else {
            let info = self.u.info(s);
            let info = self.map_type(m, info);
            self.u.set_info(n, info);
        }
        if self.u.is_deferred(s) {
            self.u.set_deferred(n, true);
        }
        for c in self.u.annotations(s).to_vec() {
            self.u.annotate(n, c);
        }
    }

    /// The owner of the children of `t`, owned by `owner`, when retyped, and the class enclosing
    /// them: the definition `t` is, the class if `t` defines one.
    pub fn owners_within(&self, t: TNodeId, owner: SymbolId) -> (SymbolId, Option<SymbolId>) {
        let inner = match self.tpd.tree(t) {
            T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Template { .. } => {
                self.tpd.sym(t).unwrap_or(owner)
            }
            _ => owner,
        };
        match matches!(self.tpd.tree(t), T::TypeDef { .. }) && self.u.symbol(inner).kind == Kind::Class {
            true => (inner, Some(inner)),
            false => (inner, self.enclosing_class),
        }
    }

    /// `tree` without the imports among its statements, if a block or a template.
    pub fn without_imports(&mut self, tree: T) -> T {
        let is_import = |i: &Self, s| matches!(i.tpd.tree(s), T::Import { .. });
        match tree {
            T::Block { stats, expr } if self.tpd.items(stats).iter().any(|&s| is_import(self, s)) => {
                let kept: Vec<_> =
                    self.tpd.items(stats).iter().copied().filter(|&s| !is_import(self, s)).collect();
                T::Block { stats: self.tpd.list(&kept), expr }
            }
            T::Template { constr, parents, self_, body }
                if self.tpd.items(body).iter().any(|&s| is_import(self, s)) =>
            {
                let kept: Vec<_> =
                    self.tpd.items(body).iter().copied().filter(|&s| !is_import(self, s)).collect();
                T::Template { constr, parents, self_, body: self.tpd.list(&kept) }
            }
            tree => tree,
        }
    }
}

/// `t` with the members of copied classes selected through a prefix renamed to their copies.
pub fn rename_members(u: &mut Universe, syms: &FxHashMap<SymbolId, SymbolId>, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, part| match *u.types().get(part) {
        Type::Sym(Some(p), s) if syms.contains_key(&s) => {
            let p = rename_members(u, syms, p);
            Some(u.intern_type(Type::Sym(Some(p), syms[&s])))
        }
        _ => None,
    })
}
