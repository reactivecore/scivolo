//! The typer as the host of inline expansions: it lends them its trees and tables, records the
//! context of each inline call and searches implicit arguments for them.

use super::Typer;
use super::implicits::is_misfit;
use crate::ctx::Ctx;
use crate::inlining::{Host, Inlining, Lent};
use crate::tpd::{TNodeId, Tree as T};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use syntax::ast::Span;
use tasty::tags::{INLINE, MACRO};
use tasty::{Kind, Type, TypeId, Universe};

impl Typer<'_> {
    /// `f` run on an inlining of this typer's trees and tables, the typer its host; with
    /// `transparent_only`, only transparent calls expand.
    pub fn with_inlining<R>(&mut self, transparent_only: bool, f: impl FnOnce(&mut Inlining) -> R) -> R {
        let mut u = std::mem::replace(&mut *self.u, Universe::empty());
        let mut tpd = std::mem::take(&mut self.tpd);
        let mut names = std::mem::take(&mut *self.names);
        let mut fresh = std::mem::take(&mut *self.fresh);
        let parts = ((&mut u, &mut tpd), (&mut names, &mut fresh));
        let result = catch_unwind(AssertUnwindSafe(|| {
            f(&mut Inlining::new(parts.0, parts.1, &mut *self, transparent_only))
        }));
        (*self.u, self.tpd, *self.names, *self.fresh) = (u, tpd, names, fresh);
        result.unwrap_or_else(|panic| resume_unwind(panic))
    }

    /// Records `ctx` as the context of `tree` if it is a call of an inline method with all its
    /// arguments, outside an inline method.
    pub fn record_inline_call(&mut self, tree: TNodeId, ctx: Ctx) {
        if let T::Block { expr, .. } = *self.tpd.tree(tree) {
            return self.record_inline_call(expr, ctx);
        }
        let Some(meth) = self.tpd.sym(self.tpd.meth_part(tree)) else { return };
        let s = self.u.symbol(meth);
        if s.kind != Kind::Def || !s.flags.has(INLINE) || s.flags.has(MACRO) {
            return;
        }
        let Some(tpe) = self.tpd.get(tree).tpe else { return };
        let widened = self.u.widen(tpe);
        if !matches!(self.u.types().get(widened), Type::Method { .. } | Type::Poly { .. })
            && self.enclosing_inline(ctx.owner).is_none()
        {
            self.inline_ctxs.insert(tree, ctx);
        }
    }

    fn swap_lent(&mut self, (u, tpd, names, fresh): Lent<'_>) {
        std::mem::swap(&mut *self.u, u);
        std::mem::swap(&mut self.tpd, tpd);
        std::mem::swap(&mut *self.names, names);
        std::mem::swap(&mut *self.fresh, fresh);
    }
}

impl Host for Typer<'_> {
    fn call_ctx(&self, call: TNodeId) -> Option<Ctx> {
        self.inline_ctxs.get(&call).copied()
    }

    fn summon(
        &mut self,
        lent: Lent<'_>,
        pt: TypeId,
        at: Span,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, &'static str> {
        let (u, tpd, names, fresh) = lent;
        self.swap_lent((&mut *u, &mut *tpd, &mut *names, &mut *fresh));
        let found = catch_unwind(AssertUnwindSafe(|| match self.implicit_arg(pt, at, ctx, 0) {
            Ok(t) => self.define_tree(t, true).map(|()| Some(t)),
            Err(what) if is_misfit(what) => Ok(None),
            Err(what) => Err(what),
        }));
        self.swap_lent((u, tpd, names, fresh));
        found.unwrap_or_else(|panic| resume_unwind(panic))
    }

    fn error(&mut self, at: Span, msg: String) {
        self.error_at(at, msg);
    }
}
