//! What an expansion asks of the typer it runs in: the context of a call, implicit arguments,
//! errors (dotty's `Inliner` working in the context of the call).

use super::{Inlining, Unsupported};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use syntax::ast::Span;
use syntax::names::{FreshNames, Names};
use tasty::tags::{GIVEN, IMPLICIT};
use tasty::{TypeId, Universe};

/// The trees and tables an expansion works on, lent back to the typer for a search.
pub type Lent<'l> = (&'l mut Universe, &'l mut Tpd, &'l mut Names, &'l mut FreshNames);

pub trait Host {
    /// The context of inline call `call`, outside an inline method, as the typer recorded it.
    fn call_ctx(&self, call: TNodeId) -> Option<Ctx>;
    /// An implicit argument of type `pt` at `at` in `ctx`, its type variables instantiated
    /// (dotty's `inferImplicitArg`); `Ok(None)` if no candidate fits.
    fn summon(
        &mut self,
        lent: Lent<'_>,
        pt: TypeId,
        at: Span,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, &'static str>;
    /// Reports error `msg` at `at`.
    fn error(&mut self, at: Span, msg: String);
}

impl Inlining<'_> {
    /// An implicit argument of type `pt` at `at` in the context of the outermost call; `Ok(None)`
    /// if there is none.
    pub fn summon(&mut self, pt: TypeId, at: Span) -> Result<Option<TNodeId>, Unsupported> {
        let Some(ctx) = self.ctx else { return Err((at, "implicit search without the call's context")) };
        if self.local_implicits {
            return Err((at, "implicit search in an inline body with local implicits"));
        }
        let lent = (&mut *self.u, &mut *self.tpd, &mut *self.names, &mut *self.fresh);
        self.host.summon(lent, pt, at, ctx).map_err(|what| (at, what))
    }

    /// Reports error `msg` at `at`.
    pub fn error(&mut self, at: Span, msg: String) {
        self.host.error(at, msg);
    }

    /// Whether the body of inline method definition `def` defines a given or implicit term other
    /// than by the pattern of a `summonFrom` case.
    pub fn defines_implicit(&self, def: TNodeId) -> bool {
        let T::DefDef { rhs: Some(rhs), .. } = *self.tpd.tree(def) else { return false };
        self.has_implicit_def(rhs)
    }

    fn has_implicit_def(&self, t: TNodeId) -> bool {
        let def = matches!(self.tpd.tree(t), T::ValDef { .. } | T::DefDef { .. });
        let implicit = self.tpd.sym(t).is_some_and(|s| {
            let flags = self.u.symbol(s).flags;
            flags.has(GIVEN) || flags.has(IMPLICIT)
        });
        def && implicit || self.tpd.children(t).into_iter().any(|c| self.has_implicit_def(c))
    }
}
