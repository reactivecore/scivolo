//! What an expansion asks of the typer it runs in: the context of a call, implicit arguments,
//! errors (dotty's `Inliner` working in the context of the call).

use super::{Expanded, Inlining, MacroCall, MacroError, Unsupported};
use crate::ctx::Ctx;
use crate::tpd::{MatchKind, TNodeId, Tpd, Tree as T};
use syntax::ast::Span;
use syntax::names::{FreshNames, Names};
use tasty::tags::{GIVEN, IMPLICIT};
use tasty::{SymbolId, TypeId, Universe};

/// The trees and tables an expansion works on, lent back to the typer for a search.
pub type Lent<'l> = (&'l mut Universe, &'l mut Tpd, &'l mut Names, &'l mut FreshNames);

pub trait Host {
    /// The context of inline call `call`, outside an inline method, as the typer recorded it.
    fn call_ctx(&self, call: TNodeId) -> Option<Ctx>;
    /// `ctx` inside a scope of `locals` by name.
    fn within(&mut self, ctx: Ctx, locals: Vec<(String, SymbolId)>) -> Ctx;
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
    /// Whether an error has been reported.
    fn has_errors(&self) -> bool;
    /// What macro call `call` expands to (dotty's `Splicer.splice`).
    fn expand_macro(&mut self, lent: Lent<'_>, call: &MacroCall) -> Result<Expanded, MacroError>;
}

impl Inlining<'_> {
    /// An implicit argument of type `pt` at `at` in the context of the outermost call; `Ok(None)`
    /// if there is none.
    pub fn summon(&mut self, pt: TypeId, at: Span) -> Result<Option<TNodeId>, Unsupported> {
        let Some(ctx) = self.ctx else { return Err((at, "implicit search without the call's context")) };
        if self.local_implicits {
            return Err((at, "implicit search in an inline body with local implicits"));
        }
        let ctx = match self.block_implicits.is_empty() {
            true => ctx,
            false => {
                let locals: Vec<(String, SymbolId)> =
                    self.block_implicits.iter().map(|&s| (self.u.symbol(s).name.clone(), s)).collect();
                self.host.within(ctx, locals)
            }
        };
        let lent = (&mut *self.u, &mut *self.tpd, &mut *self.names, &mut *self.fresh);
        self.host.summon(lent, pt, at, ctx).map_err(|what| (at, what))
    }

    /// Reports error `msg` at `at`.
    pub fn error(&mut self, at: Span, msg: String) {
        self.host.error(at, msg);
    }

    /// Whether the body of inline method definition `def` defines a given or implicit term other
    /// than by the pattern of a `summonFrom` case; a macro's body, which its expansion replaces,
    /// does not.
    pub fn defines_implicit(&self, def: TNodeId) -> bool {
        let T::DefDef { rhs: Some(rhs), .. } = *self.tpd.tree(def) else { return false };
        if self.tpd.sym(def).is_some_and(|s| self.u.symbol(s).flags.has(tasty::tags::MACRO)) {
            return false;
        }
        self.has_implicit_def(rhs)
    }

    /// Whether tree `t` defines a given or implicit term other than by a block statement, which
    /// `block_implicits` tracks, outside the cases of inline matches, which count once their
    /// match selects them.
    pub fn has_implicit_def(&self, t: TNodeId) -> bool {
        match *self.tpd.tree(t) {
            T::Match { selector, kind: MatchKind::Inline, .. } => {
                return selector.is_some_and(|s| self.has_implicit_def(s));
            }
            T::Block { stats, expr } => {
                let stats = self.tpd.items(stats);
                let inner = |s: TNodeId| self.tpd.children(s).into_iter().any(|c| self.has_implicit_def(c));
                return stats
                    .iter()
                    .any(|&s| if self.is_implicit_def(s) { inner(s) } else { self.has_implicit_def(s) })
                    || self.has_implicit_def(expr);
            }
            _ => {}
        }
        self.is_implicit_def(t) || self.tpd.children(t).into_iter().any(|c| self.has_implicit_def(c))
    }

    /// Adds the implicits the statements of `t` define to `block_implicits` if it is a block;
    /// their number before.
    pub fn enter_block_implicits(&mut self, t: TNodeId) -> usize {
        let mark = self.block_implicits.len();
        if let T::Block { stats, .. } = *self.tpd.tree(t) {
            let stats = self.tpd.items(stats).to_vec();
            let defined =
                stats.into_iter().filter(|&s| self.is_implicit_def(s)).filter_map(|s| self.tpd.sym(s));
            self.block_implicits.extend(defined.collect::<Vec<_>>());
        }
        mark
    }

    /// Whether `t` defines a given or implicit value or method.
    pub fn is_implicit_def(&self, t: TNodeId) -> bool {
        let def = matches!(self.tpd.tree(t), T::ValDef { .. } | T::DefDef { .. });
        def && self.tpd.sym(t).is_some_and(|s| {
            let flags = self.u.symbol(s).flags;
            flags.has(GIVEN) || flags.has(IMPLICIT)
        })
    }
}
