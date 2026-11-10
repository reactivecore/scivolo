// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! `using` clauses of calls and of partial applications after lifted arguments.

use super::super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use tasty::Type;

impl Typer<'_> {
    /// Interpolates the type variables of id `from` or later again and zonks the type if
    /// `adapted`, the adaptation of `tree` to `pt`, applies it (dotty's `readaptSimplified`).
    pub fn reinterpolate(
        &mut self,
        (tree, adapted): (TNodeId, TNodeId),
        pt: Pt,
        from: usize,
    ) -> Result<(), &'static str> {
        match *self.tpd.tree(adapted) {
            T::Apply { fun, .. }
                if adapted != tree && self.tpd.meth_part(fun) == self.tpd.meth_part(tree) =>
            {
                self.interpolate_zonked(adapted, pt, from)
            }
            _ => Ok(()),
        }
    }

    /// Applies function `f` to the arguments of its leading `using` clauses, unless the call
    /// passes them itself as in `f(using x)`, flagged by `using` (dotty's `adaptToArgs` without a
    /// `matchingApply`).
    pub fn with_using_args(
        &mut self,
        mut f: TNodeId,
        using: bool,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        while !using && self.takes_using(f) {
            f = self.implicit_clause_applied(f, ctx)?;
            f = self.with_type_vars(f)?;
        }
        Ok(f)
    }

    /// Checks `tree`, the partial application of a call whose arguments were lifted into `defs`,
    /// as `f(g())` of `def f(a: A)(b: B)`: it is kept where a function is expected or only an
    /// implicit or `using` clause follows with its type fully defined, and refused otherwise.
    pub fn lifted_partial(
        &mut self,
        tree: TNodeId,
        defs: &[TNodeId],
        pt: Pt,
    ) -> Result<TNodeId, &'static str> {
        let tpe = self.tpd.tpe(tree);
        if defs.is_empty() || !matches!(self.u.types().get(tpe), Type::Method { .. }) {
            return Ok(tree);
        }
        if matches!(pt, Pt::Fun(_)) {
            return Ok(tree);
        }
        let implicit = matches!(self.u.types().get(tpe), Type::Method { implicit: true, .. });
        if matches!(pt, Pt::PolyFun(_)) || !(implicit || self.takes_using(tree)) || self.is_open(tpe) {
            return Err("lifted call of a curried method");
        }
        Ok(tree)
    }

    /// The definitions and the method of `f`, a partial application after its lifted arguments
    /// as `lifted_partial` leaves it, or none and `f`.
    pub fn lifted_fun(&mut self, f: TNodeId) -> (Vec<TNodeId>, TNodeId) {
        match *self.tpd.tree(f) {
            T::Block { stats, expr }
                if matches!(self.u.types().get(self.tpd.tpe(expr)), Type::Method { .. }) =>
            {
                (self.tpd.items(stats).to_vec(), expr)
            }
            _ => (Vec::new(), f),
        }
    }

    /// Whether the first parameter clause of function `f` is a `using` clause.
    pub fn takes_using(&mut self, f: TNodeId) -> bool {
        let tpe = self.tpd.tpe(f);
        let info = self.method_info(tpe).unwrap_or(tpe);
        matches!(self.u.types().get(info), Type::Method { contextual: true, .. })
    }
}
