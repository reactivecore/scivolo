// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//! The children of a sealed class seen through the prefix its sum mirror is summoned at
//! (dotty's `whyNotGenericSum` scope check and `TypeOps.childPrefix`).

use super::super::Typer;
use crate::sealed;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The classes of prefix `pre` if it is a singleton type (dotty's `ownerScope`).
    pub fn owner_scope(&mut self, pre: Option<TypeId>) -> Vec<SymbolId> {
        let Some(p) = pre else { return Vec::new() };
        let singleton = match *self.u.types().get(p) {
            Type::This(_) => true,
            Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
            _ => false,
        };
        let widened = self.u.widen(p);
        singleton.then(|| self.u.class_symbol(widened)).flatten().into_iter().collect()
    }

    /// Whether `owner` encloses `site`, is inherited by a class of `scope`, or is a package or an
    /// object visible so (dotty's `isVisibleToScope`).
    pub fn is_visible_to_scope(&mut self, owner: SymbolId, site: SymbolId, scope: &[SymbolId]) -> bool {
        self.u.symbol(owner).kind == Kind::Package
            || sealed::is_contained_in(self.u, site, owner)
            || scope.iter().any(|&c| self.inherits(owner, c))
            || self.u.is_module_class(owner)
                && self.u.symbol(owner).owner.is_some_and(|o| self.is_visible_to_scope(o, site, scope))
    }

    /// Whether class `scope` or one of its owners up to a package derives from `sym`.
    fn inherits(&mut self, sym: SymbolId, scope: SymbolId) -> bool {
        self.u.symbol(scope).kind != Kind::Package
            && (self.u.is_class(scope) && self.u.derives_from(scope, sym)
                || self.u.symbol(scope).owner.is_some_and(|o| self.inherits(sym, o)))
    }

    /// The prefix of child `child` of sealed class `parent` referred to through `pre`: the part of
    /// `pre` the owners of both share, with the child's other owners selected from it.
    pub fn child_prefix(&mut self, pre: TypeId, parent: SymbolId, child: SymbolId) -> Option<TypeId> {
        let owners = |t: &Self, s: SymbolId| -> Vec<SymbolId> {
            let mut v: Vec<SymbolId> =
                std::iter::successors(t.u.symbol(s).owner, |&o| t.u.symbol(o).owner).collect();
            v.reverse();
            v
        };
        let (parents, children) = (owners(self, parent), owners(self, child));
        let common = parents.iter().zip(&children).take_while(|(a, b)| a == b).count();
        let mut prefix = self.sub_prefix_at(pre, parents.len() - common)?;
        for &s in &children[common..] {
            let s = match self.u.is_module_class(s) {
                true => self.u.source_module(s)?,
                false => s,
            };
            let p = (!matches!(self.u.types().get(prefix), Type::NoPrefix)).then_some(prefix);
            prefix = self.u.intern_type(Type::Sym(p, s));
        }
        Some(prefix)
    }

    /// The prefix `limit` selections out of `pre`; `None` for a prefix of another form.
    fn sub_prefix_at(&mut self, pre: TypeId, limit: usize) -> Option<TypeId> {
        if limit == 0 {
            return Some(pre);
        }
        match self.u.types().get(pre).clone() {
            Type::Sym(Some(p), s) if self.u.symbol(s).kind.is_term() => self.sub_prefix_at(p, limit - 1),
            Type::Sym(None, s) if self.u.symbol(s).kind.is_term() && limit == 1 => {
                Some(self.u.intern_type(Type::NoPrefix))
            }
            _ => None,
        }
    }
}
