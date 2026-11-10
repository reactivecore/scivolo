// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Where a view search looks, and the types it does not convert from or to.

use super::super::super::Typer;
use super::super::super::type_ops::TypeOps;
use super::super::scope::ImplicitScope;
use super::{Goal, ViewSearch};
use tasty::{Kind, Type, TypeId};

impl ViewSearch<'_> {
    /// Collects the implicit scope of a view of `from` to the goal, with the containers of
    /// extension methods in that of `from`; fails when the scope holds an extension method of the
    /// selected name itself.
    pub fn scope(&self, t: &mut Typer<'_>, from: TypeId) -> Result<ImplicitScope, &'static str> {
        let mut scope = ImplicitScope::of(t, from)?;
        if let Goal::Conform(pt) = self.goal {
            scope.implicits.extend(t.implicit_scope(pt)?);
        }
        if let Some(name) = self.goal.member()
            && scope.implicits.iter().any(|c| {
                let s = t.u.symbol(c.sym);
                s.name == name && s.is_extension()
            })
        {
            return Err("extension method in the implicit scope");
        }
        Ok(scope)
    }
}

impl TypeOps<'_> {
    /// Tells whether `pt` is `Any`, `AnyVal`, `AnyRef`, `Matchable` or `Unit`, which no
    /// conversion targets; not an abstract type bounded by one of them (dotty's `isRef`).
    pub fn is_top_like(&mut self, pt: TypeId) -> bool {
        let pt = self.zonk(pt);
        let pt = self.u.dealias(pt);
        if matches!(*self.u.types().get(pt), Type::Flexible(_))
            || matches!(*self.u.types().get(pt), Type::Sym(_, s) if matches!(self.u.symbol(s).kind, Kind::Type | Kind::TypeParam))
        {
            return false;
        }
        let b = self.u.builtins();
        let class = self.u.class_symbol(pt);
        class.is_some_and(|c| {
            c == b.any
                || c == b.object
                || c == b.matchable
                || Some(c) == b.any_val
                || self.u.full_name(c) == "scala.Unit"
        })
    }

    /// Tells whether `from` is `Null` or `Nothing`, which no conversion takes
    /// (`isUnderSpecifiedArgument`).
    pub fn is_underspecified_arg(&mut self, from: TypeId) -> bool {
        let widened = self.u.widen(from);
        let b = self.u.builtins();
        self.u.class_symbol(widened).is_some_and(|c| c == b.nothing || c == b.null)
    }
}
