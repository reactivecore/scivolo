// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Where a view search looks, and the types it does not convert from or to.

use super::super::Typer;
use super::candidates::Candidate;
use super::conversions::Goal;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// The implicit scope of a view of `from` to `goal`, and the containers of extension methods
    /// in that of `from`.
    pub fn view_scope(
        &mut self,
        from: TypeId,
        goal: Goal,
    ) -> Result<(Vec<Candidate>, Vec<Candidate>), &'static str> {
        let super::scope::ImplicitScope { implicits: mut scope, containers } =
            self.implicit_scope_of(from)?;
        if let Goal::Conform(pt) = goal {
            scope.extend(self.implicit_scope(pt)?);
        }
        if let Goal::Member(name) = goal
            && scope.iter().any(|c| {
                let s = self.u.symbol(c.sym);
                s.name == name && s.flags.has(tasty::tags::EXTENSION)
            })
        {
            return Err("extension method in the implicit scope");
        }
        Ok((scope, containers))
    }

    /// Whether `pt` is `Any`, `AnyVal`, `AnyRef`, `Matchable` or `Unit`, no conversion's target;
    /// not an abstract type bounded by one of them (dotty's `isRef`).
    pub fn is_top_like(&mut self, pt: TypeId) -> bool {
        let pt = self.zonk(pt);
        let pt = self.u.dealias(pt);
        if matches!(*self.u.types().get(pt), Type::Sym(_, s) if matches!(self.u.symbol(s).kind, Kind::Type | Kind::TypeParam))
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

    /// Whether `from` is `Null` or `Nothing`, which no conversion takes (`isUnderSpecifiedArgument`).
    pub fn is_underspecified_arg(&mut self, from: TypeId) -> bool {
        let widened = self.u.widen(from);
        let b = self.u.builtins();
        self.u.class_symbol(widened).is_some_and(|c| c == b.nothing || c == b.null)
    }
}
