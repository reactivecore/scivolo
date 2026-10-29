//! The typer as the host of inline expansions: it lends them its trees and tables, records the
//! context of each inline call and searches implicit arguments for them.

use super::Typer;
use super::implicits::is_misfit;
use crate::ctx::{Ctx, Level, Mode};
use crate::inlining::{
    AnnotationCall, Annotations, Expanded, Host, Inlining, Lent, MacroCall, MacroError, Macros,
};
use crate::tpd::{TNodeId, Tree as T};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use syntax::ast::Span;
use tasty::tags::{INLINE, MACRO};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

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
        if s.kind != Kind::Def || !s.flags.has(INLINE) || s.flags.has(MACRO) && self.macros.is_none() {
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

    /// Records `ctx`, out of pattern mode, as the context of extractor `fun` if it is an inline
    /// method outside an inline method.
    pub fn record_inline_extractor(&mut self, fun: TNodeId, ctx: Ctx) {
        let Some(meth) = self.tpd.sym(self.tpd.meth_part(fun)) else { return };
        if self.u.symbol(meth).flags.has(INLINE) && self.enclosing_inline(ctx.owner).is_none() {
            self.inline_ctxs.insert(fun, ctx.with_mode(ctx.mode.without(Mode::PATTERN)));
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

    fn within(&mut self, ctx: Ctx, locals: Vec<(String, SymbolId)>) -> Ctx {
        Ctx { level: self.levels.push(Some(ctx.level), Level::Scope(locals)), ..ctx }
    }

    fn copied_call(&mut self, original: TNodeId, copy: TNodeId) {
        if let Some(&ctx) = self.inline_ctxs.get(&original) {
            self.inline_ctxs.insert(copy, ctx);
        }
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

    fn has_errors(&self) -> bool {
        self.diags.has_errors()
    }

    fn error_count(&self) -> usize {
        self.diags.items.len()
    }

    fn errors_since(&mut self, mark: usize) -> bool {
        let errors = self.diags.items[mark..].iter().any(|d| d.level == syntax::diag::Level::Error);
        self.diags.items.truncate(mark);
        errors
    }

    fn typed_code(&mut self, lent: Lent<'_>, code: &str, ctx: Ctx) -> Option<TNodeId> {
        let (u, tpd, names, fresh) = lent;
        self.swap_lent((&mut *u, &mut *tpd, &mut *names, &mut *fresh));
        let typed = catch_unwind(AssertUnwindSafe(|| self.typed_code_in(code, ctx)));
        self.swap_lent((u, tpd, names, fresh));
        typed.unwrap_or_else(|panic| resume_unwind(panic))
    }

    fn inline_def(&mut self, lent: Lent<'_>, meth: SymbolId) -> Result<Option<TNodeId>, &'static str> {
        let (u, tpd, names, fresh) = lent;
        self.swap_lent((&mut *u, &mut *tpd, &mut *names, &mut *fresh));
        let def = catch_unwind(AssertUnwindSafe(|| self.unit_inline_def(meth)));
        self.swap_lent((u, tpd, names, fresh));
        def.unwrap_or_else(|panic| resume_unwind(panic))
    }

    fn expand_macro(&mut self, lent: Lent<'_>, call: &MacroCall) -> Result<Expanded, MacroError> {
        self.with_macros(lent, call.span, |m, lent, annotations| m.expand(lent, call, annotations))
    }

    fn annotations(&self, s: SymbolId) -> Vec<TNodeId> {
        self.syms.annotations.get(&s).cloned().unwrap_or_default()
    }

    fn expand_annotation(&mut self, lent: Lent<'_>, call: &AnnotationCall) -> Result<Expanded, MacroError> {
        self.with_macros(lent, call.span, |m, lent, annotations| m.expand_annotation(lent, call, annotations))
    }
}

impl Typer<'_> {
    /// What `run` answers with the unit's annotations typed; a refusal at `span` recorded.
    fn with_macros(
        &mut self,
        (u, tpd, names, fresh): Lent<'_>,
        span: Span,
        run: impl FnOnce(&mut dyn Macros, Lent<'_>, &Annotations) -> Result<Expanded, MacroError>,
    ) -> Result<Expanded, MacroError> {
        if self.macros.is_some() {
            self.swap_lent((&mut *u, &mut *tpd, &mut *names, &mut *fresh));
            let typed = catch_unwind(AssertUnwindSafe(|| self.typed_annotations()));
            self.swap_lent((&mut *u, &mut *tpd, &mut *names, &mut *fresh));
            typed.unwrap_or_else(|panic| resume_unwind(panic));
        }
        let expanded = match self.macros.as_deref_mut() {
            Some(macros) => run(macros, (u, tpd, names, fresh), &self.syms.annotations),
            None => Err(MacroError::Unsupported("macro call".into())),
        };
        if let Err(MacroError::Unsupported(what)) = &expanded {
            self.unsupported_at(span, what);
        }
        expanded
    }
}
