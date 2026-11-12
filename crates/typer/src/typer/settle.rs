// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The types of typed trees settled: type variables replaced, types simplified (dotty's
//! `Typer.simplify`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// The type of `tree` typed in `ctx` with its instantiated type variables replaced, and
    /// simplified unless it is a typed pattern.
    pub fn settle_type(&mut self, tree: TNodeId, ctx: Ctx) {
        self.zonk_type(tree);
        if !(ctx.mode.is(Mode::PATTERN) && matches!(self.tpd.tree(tree), T::Typed { .. })) {
            self.simplify_tree_type(tree, ctx);
        }
    }

    /// The type variables created while typing `tree` interpolated and its type settled (dotty's
    /// `simplify` of a tree built by `adapt`).
    pub fn settle_adapted(&mut self, tree: TNodeId, pt: Pt, ctx: Ctx) {
        if let Err(what) = self.interpolate(tree, pt, self.locked_vars) {
            let span = self.tpd.span(tree);
            self.unsupported_at(span, what);
        }
        self.settle_type(tree, ctx);
    }

    /// `interpolate`, then the type of `tree` zonked (dotty's `simplify` of an application).
    pub fn interpolate_zonked(&mut self, tree: TNodeId, pt: Pt, from: usize) -> Result<(), &'static str> {
        self.interpolate(tree, pt, from)?;
        self.zonk_type(tree);
        Ok(())
    }

    /// The type of `tree` with its instantiated type variables replaced.
    pub fn zonk_type(&mut self, tree: TNodeId) {
        let Some(t) = self.tpd.get(tree).tpe else { return };
        let t = self.zonk(t);
        self.tpd.get_mut(tree).tpe = Some(t);
    }

    /// The type of term tree `tree` simplified and its match types reduced, unless it is a
    /// method's or a match type that reduces using a type parameter `ctx` may narrow (dotty's
    /// `simplify` of a typed tree).
    fn simplify_tree_type(&mut self, tree: TNodeId, ctx: Ctx) {
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
        if is_type || matches!(self.u.ty(widened), Type::Method { .. } | Type::Poly { .. }) {
            return;
        }

        let t = self.with_match_alias_vars_defined(t);
        let simplified = self.simplified_inferred(t);
        let kept = if self.reduces_using_gadt(t, ctx) { t } else { simplified };
        self.tpd.get_mut(tree).tpe = Some(kept);
    }

    /// `t` zonked after instantiating the type variables of each applied match alias in it
    /// (dotty's `isFullyDefined` with `ForceDegree.all` in `TypeOps.simplify`).
    fn with_match_alias_vars_defined(&mut self, t: TypeId) -> TypeId {
        if !self.types().has_type_var(t) {
            return t;
        }
        let mut aliases = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if u.applied(part).is_some() && u.underlying_match(part).is_some() {
                aliases.push(part);
            }
            None
        });
        for part in aliases {
            if self.types().has_type_var(part) {
                let _ = self.fully_defined_as(part, true);
            }
        }
        self.zonk(t)
    }

    /// Whether `t` is a match type whose scrutinee refers to a type parameter a case typed in
    /// `ctx` may narrow (dotty's `MatchType.thatReducesUsingGadt`).
    fn reduces_using_gadt(&mut self, t: TypeId, ctx: Ctx) -> bool {
        let Some(m) = self.u.underlying_match(t) else { return false };
        let Type::Match { scrutinee, .. } = *self.u.ty(m) else { return false };
        let mut params = Vec::new();
        self.u.map_type(scrutinee, &mut |u, part| {
            if let Some(s) = u.sym_of(part).filter(|&s| u.symbol(s).kind == Kind::TypeParam) {
                params.push(s);
            }
            None
        });
        params.into_iter().any(|s| self.gadt.is_symbol(self.u, s, ctx))
    }
}
