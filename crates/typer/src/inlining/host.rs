//! What an expansion asks of the typer it runs in: the context of a call, implicit arguments,
//! errors (dotty's `Inliner` working in the context of the call).

use super::{Expanded, Inlining, MacroCall, MacroError, Unsupported};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use syntax::ast::Span;
use syntax::names::{FreshNames, Names};
use tasty::tags::{GIVEN, IMPLICIT};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

/// The trees and tables an expansion works on, lent back to the typer for a search.
pub type Lent<'l> = (&'l mut Universe, &'l mut Tpd, &'l mut Names, &'l mut FreshNames);

pub trait Host {
    /// The context of inline call `call`, outside an inline method, as the typer recorded it.
    fn call_ctx(&self, call: TNodeId) -> Option<Ctx>;
    /// Gives `copy` the context of call `original`, if it has one.
    fn copied_call(&mut self, original: TNodeId, copy: TNodeId);
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
        let outer = std::mem::take(&mut self.outer_implicits);
        let fits_outer = self.may_fit(&outer, pt);
        self.outer_implicits = outer;
        if self.local_implicits || fits_outer {
            return Err((at, "implicit search in an inline body with local implicits"));
        }
        let fitting: Vec<SymbolId> = (self.case_givens.clone().into_iter())
            .filter(|&g| {
                let info = self.u.info(g);
                self.u.is_subtype(info, pt)
            })
            .collect();
        match fitting[..] {
            [] => {}
            [g] => {
                let (name, tpe) = (self.term_name(g), self.u.intern_type(Type::Sym(None, g)));
                return Ok(Some(self.tpd.node(T::Ident { name }, at.to_synthetic(), Some(tpe), Some(g))));
            }
            _ => return Err((at, "implicit search fitting several givens of summonFrom cases")),
        }
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
        let mut found = Vec::new();
        self.implicit_defs(rhs, &mut found);
        !found.is_empty()
    }

    /// Adds the given and implicit terms `t` defines to `found`.
    pub fn implicit_defs(&self, t: TNodeId, found: &mut Vec<SymbolId>) {
        if matches!(self.tpd.tree(t), T::ValDef { .. } | T::DefDef { .. })
            && let Some(s) = self.tpd.sym(t)
            && (self.u.symbol(s).flags.has(GIVEN) || self.u.symbol(s).flags.has(IMPLICIT))
        {
            found.push(s);
        }
        for c in self.tpd.children(t) {
            self.implicit_defs(c, found);
        }
    }

    /// Whether one of the implicits `defs` of a typer's expansion around the call may be an
    /// argument of type `pt`: a method, or a value of a subtype of `pt`.
    fn may_fit(&mut self, defs: &[SymbolId], pt: TypeId) -> bool {
        defs.iter().any(|&d| {
            let info = self.u.info(d);
            self.u.symbol(d).kind != Kind::Val || self.u.is_subtype(info, pt)
        })
    }
}
