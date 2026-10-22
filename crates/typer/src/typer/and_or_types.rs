// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//! Intersection and union types `A & B`, `A | B`: dotty's aliases `scala.&` and `scala.|`
//! (`Definitions.andType`, `orType`), applied as `AndType` and `OrType`.

use super::Typer;
use crate::ctx::{Ctx, Mode};
use crate::defn::entered;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Enters `scala.&` and `scala.|` unless the universe has them.
    pub fn enter_and_or(&mut self) {
        for (name, and) in [("&", true), ("|", false)] {
            if entered(self.u, self.defn.scala, name, Kind::Type).is_some() {
                continue;
            }
            let s =
                self.syms.define(self.u, name, Kind::Type, self.defn.scala, syntax::ast::flags::SYNTHETIC);
            let binder = self.u.fresh_binder();
            let bounds = self.u.intern_type(Type::Bounds(tasty::NOTHING, tasty::ANY));
            let params = vec![("A".to_owned(), bounds), ("B".to_owned(), bounds)];
            let (a, b) =
                (self.u.intern_type(Type::Param(binder, 0)), self.u.intern_type(Type::Param(binder, 1)));
            let result = self.u.intern_type(if and { Type::And(a, b) } else { Type::Or(a, b, false) });
            let lambda = self.u.intern_type(Type::Lambda { binder, params, result });
            let alias = self.u.intern_type(Type::Alias(lambda));
            self.u.set_info(s, alias);
        }
    }

    /// The type of `tree` typed in `ctx` with its instantiated type variables replaced, and
    /// simplified outside patterns.
    pub fn settle_type(&mut self, tree: TNodeId, ctx: Ctx) {
        let Some(t) = self.tpd.get(tree).tpe else { return };
        let t = self.zonk(t);
        self.tpd.get_mut(tree).tpe = Some(t);
        if !ctx.mode.is(Mode::PATTERN) {
            self.simplify_tree_type(tree);
        }
    }

    /// The type of term tree `tree` simplified, unless it is a method's (dotty's `simplify` of a
    /// typed tree).
    fn simplify_tree_type(&mut self, tree: TNodeId) {
        let is_type = match *self.tpd.tree(tree) {
            T::Ident { name } | T::Select { name, .. } => name.is_type,
            T::TypeTree { .. }
            | T::SingletonTypeTree { .. }
            | T::ByNameTypeTree { .. }
            | T::Annotated { .. }
            | T::AppliedTypeTree { .. }
            | T::RefinedTypeTree { .. }
            | T::TypeBoundsTree { .. } => true,
            _ => false,
        };
        let Some(t) = self.tpd.get(tree).tpe else { return };
        let widened = self.u.widen(t);
        if is_type || matches!(self.u.types().get(widened), Type::Method { .. } | Type::Poly { .. }) {
            return;
        }
        let simplified = self.simplified(t);
        self.tpd.get_mut(tree).tpe = Some(simplified);
    }

    /// `a & b` if `alias` is `scala.&`, `a | b` if it is `scala.|`.
    pub fn and_or_applied(&mut self, alias: SymbolId, a: TypeId, b: TypeId) -> Option<TypeId> {
        if self.u.symbol(alias).owner != Some(self.defn.scala) || self.u.symbol(alias).kind != Kind::Type {
            return None;
        }
        let t = match self.u.symbol(alias).name.as_str() {
            "&" => Type::And(a, b),
            "|" => Type::Or(a, b, false),
            _ => return None,
        };
        Some(self.u.intern_type(t))
    }
}
