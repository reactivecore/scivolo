// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//! Intersection and union types `A & B`, `A | B`: dotty's aliases `scala.&` and `scala.|`
//! (`Definitions.andType`, `orType`), applied as `AndType` and `OrType`.

use super::Typer;
use crate::special::entered;
use names::nme;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Enters `scala.&` and `scala.|` unless the universe has them.
    pub fn enter_and_or(&mut self) {
        for (name, and) in [(nme::AND, true), (nme::OR, false)] {
            if entered(self.u, self.defn.scala, name, Kind::Type).is_some() {
                continue;
            }
            let s =
                self.syms.define(self.u, name, Kind::Type, self.defn.scala, syntax::ast::flags::SYNTHETIC);
            let binder = self.u.fresh_binder();
            let bounds = self.u.intern_type(Type::Bounds(tasty::NOTHING, tasty::ANY));
            let params = [(self.u.intern_name("X0"), bounds), (self.u.intern_name("X1"), bounds)];
            let (a, b) =
                (self.u.intern_type(Type::Param(binder, 0)), self.u.intern_type(Type::Param(binder, 1)));
            let result = self.u.intern_type(if and { Type::And(a, b) } else { Type::Or(a, b, false) });
            let lambda = self.u.intern_lambda(binder, &params, result);
            let alias = self.u.intern_type(Type::Alias(lambda));
            self.u.set_info(s, alias);
        }
    }

    /// `a & b` if `alias` is `scala.&`, `a | b` if it is `scala.|`.
    pub fn and_or_applied(&mut self, alias: SymbolId, a: TypeId, b: TypeId) -> Option<TypeId> {
        if self.u.symbol(alias).owner != Some(self.defn.scala) || self.u.symbol(alias).kind != Kind::Type {
            return None;
        }
        let t = match self.u.symbol(alias).name.as_str() {
            nme::AND => Type::And(a, b),
            nme::OR => Type::Or(a, b, false),
            _ => return None,
        };
        Some(self.u.intern_type(t))
    }
}
