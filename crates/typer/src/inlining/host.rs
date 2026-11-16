//! What an expansion asks of the typer it runs in: the context of a call, implicit arguments,
//! errors (dotty's `Inliner` working in the context of the call).

use super::local_implicits::Scope;
use super::{AnnotationCall, Expanded, Inlining, MacroCall, MacroError, Reports, Unsupported};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use syntax::ast::Span;
use syntax::diag::Level;
use syntax::names::{FreshNames, Names};
use tasty::{Kind, SymbolId, TypeId, Universe};

const ABORTED: &str = "Macro expansion was aborted by the macro without any errors reported. Macros should issue \
                       errors to end-users when aborting a macro expansion with StopMacroExpansion.";

/// The trees and tables an expansion works on, lent back to the typer for a search.
pub type Lent<'l> = (&'l mut Universe, &'l mut Tpd, &'l mut Names, &'l mut FreshNames);

/// The errors of the code of `typeChecks`: the messages and spans of those inline expansions
/// reported; `None` if the parser or the typer reported one.
pub type CheckedErrors = Option<Vec<(Span, String)>>;

/// An import selector: the name it imports (empty for `given`), the name it is visible as (`_`
/// for none), and the bound of a `given T`.
pub type ImportSelector = (String, String, Option<TNodeId>);

pub trait Host {
    /// The context of inline call `call`, outside an inline method, as the typer recorded it.
    fn call_ctx(&self, call: TNodeId) -> Option<Ctx>;
    /// `ctx` inside a scope of `locals` by name.
    fn within(&mut self, ctx: Ctx, locals: Vec<(String, SymbolId)>) -> Ctx;
    /// `ctx` inside an import of typed path `expr` of type `qual`, of selectors `(name, rename,
    /// bound)`.
    fn within_import(
        &mut self,
        ctx: Ctx,
        qual_expr: (TypeId, TNodeId),
        selectors: Vec<ImportSelector>,
    ) -> Ctx;
    /// Gives `copy` the context of call or sequence literal with spreads `original`, if it has one.
    fn copied_call(&mut self, original: TNodeId, copy: TNodeId);
    /// For `search` `(pt, ignored)`, an implicit argument of type `pt` at `at` in `ctx`, its type
    /// variables instantiated, candidates `ignored` left out (dotty's `inferImplicitArg`);
    /// `Ok(None)` if no candidate fits.
    fn summon(
        &mut self,
        lent: Lent<'_>,
        search: (TypeId, &[SymbolId]),
        at: Span,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, &'static str>;
    /// Reports error `msg` at `at`.
    fn error(&mut self, at: Span, msg: String);
    /// Reports warning `msg` at `at`.
    fn warning(&mut self, at: Span, msg: String);
    /// Whether an error has been reported.
    fn has_errors(&self) -> bool;
    /// Determines the index of the file of the unit that `at` is in.
    fn file_of(&self, at: Span) -> usize;
    /// How many diagnostics and errors of expansions have been reported (for `typeChecks`).
    fn error_count(&self) -> (usize, usize);
    /// The errors reported since `mark`, which it takes back.
    fn errors_since(&mut self, mark: (usize, usize)) -> CheckedErrors;
    /// Statements `code` parsed and typed as a block in `ctx`, or the errors that reports
    /// (dotty's `Inlines.Intrinsics.compileForErrors` up to the typer).
    fn typed_code(&mut self, lent: Lent<'_>, code: &str, ctx: Ctx) -> Result<TNodeId, CheckedErrors>;
    /// The typed definition of inline method `meth` of the unit, `Ok(None)` if it has none.
    fn inline_def(&mut self, lent: Lent<'_>, meth: SymbolId) -> Result<Option<TNodeId>, &'static str>;
    /// What macro call `call` expands to (dotty's `Splicer.splice`).
    fn expand_macro(&mut self, lent: Lent<'_>, call: &MacroCall) -> Result<Expanded, MacroError>;
    /// The typed annotations on `s`, a symbol of the unit, in source order.
    fn annotations(&self, s: SymbolId) -> Vec<TNodeId>;
    /// Gives `copy`, a fresh copy of symbol `s`, the annotations of `s`.
    fn copied_symbol(&mut self, s: SymbolId, copy: SymbolId);
    /// The definitions macro annotation call `call` returns (dotty's `MacroAnnotations.callMacro`).
    fn expand_annotation(&mut self, lent: Lent<'_>, call: &AnnotationCall) -> Result<Expanded, MacroError>;
}

impl Inlining<'_> {
    /// Sets the context of outermost call `call` and the implicits around it: the call's own, or
    /// that of the typer's `Inlined` tree around it with the scopes of the implicits that tree
    /// defines.
    pub fn enter_outermost_call(&mut self, call: TNodeId) {
        self.outer_implicits.clear();
        self.outer_scopes.clear();
        if self.transparent_only {
            return;
        }
        self.ctx = self.host.call_ctx(call);
        let Some((ctx, around)) = self.typer_ctx.filter(|_| self.ctx.is_none()) else { return };
        self.ctx = Some(ctx);
        match super::implicit_scopes_at(self.tpd, self.u, (around, call)) {
            Some(scopes) => self.outer_scopes = scopes.into_iter().map(Scope::Implicits).collect(),
            None => {
                let mut implicits = Vec::new();
                self.implicit_defs(around, &mut implicits);
                self.outer_implicits = implicits;
            }
        }
    }

    /// An implicit argument of type `pt` at `at` in the context of the outermost call; `Ok(None)`
    /// if there is none. Refused where implicits the expansion defines may take part.
    pub fn summon(&mut self, pt: TypeId, at: Span) -> Result<Option<TNodeId>, Unsupported> {
        self.summon_ignoring(pt, at, (&[], &[]))
    }

    /// `summon` with the candidates `ignored` left out, in the nested scopes, outermost first, of
    /// the givens and implicits `defined` inside the expansion.
    pub fn summon_ignoring(
        &mut self,
        pt: TypeId,
        at: Span,
        (ignored, defined): (&[SymbolId], &[Vec<SymbolId>]),
    ) -> Result<Option<TNodeId>, Unsupported> {
        let Some(ctx) = self.ctx else { return Err((at, "implicit search without the call's context")) };
        let outer = std::mem::take(&mut self.outer_implicits);
        let fits_outer = self.may_fit(&outer, pt);
        self.outer_implicits = outer;
        if self.local_implicits || fits_outer {
            return Err((at, "implicit search in an inline body with local implicits"));
        }
        let mut ctx = ctx;
        if let Some(owner) = defined.iter().flatten().next_back().and_then(|&d| self.u.symbol(d).owner) {
            ctx = ctx.with_owner(owner);
        }
        let defined = defined.iter().filter(|d| !d.is_empty()).map(|d| Scope::Implicits(d.clone()));
        let scopes = self.outer_scopes.iter().chain(&self.scopes).cloned().collect::<Vec<_>>();
        for scope in scopes.into_iter().chain(defined) {
            ctx = self.within_scope(ctx, scope);
        }
        let lent = (&mut *self.u, &mut *self.tpd, &mut *self.names, &mut *self.fresh);
        let found = self.host.summon(lent, (pt, ignored), at, ctx).map_err(|what| (at, what))?;
        let locals = self.outer_implicits.clone();
        match found {
            _ if locals.is_empty() => Ok(found),
            Some(f) if self.unshadowed_ref(f, &locals) => Ok(found),
            _ => Err((at, "implicit search in an inline body with local implicits")),
        }
    }

    /// Whether `t` is a reference whose name none of `locals` has, applied to such references
    /// as implicit arguments none of `locals` may be.
    fn unshadowed_ref(&mut self, t: TNodeId, locals: &[SymbolId]) -> bool {
        match *self.tpd.tree(t) {
            T::TypeApply { fun, .. } => self.unshadowed_ref(fun, locals),
            T::Apply { fun, args } => {
                let args = self.tpd.items(args).to_vec();
                self.unshadowed_ref(fun, locals)
                    && args.into_iter().all(|a| {
                        let tpe = self.tpd.tpe(a);
                        let tpe = self.u.widen(tpe);
                        !self.may_fit(locals, tpe) && self.unshadowed_ref(a, locals)
                    })
            }
            T::Ident { .. } | T::Select { .. } => self.tpd.sym(t).is_some_and(|s| {
                let name = &self.u.symbol(s).name;
                locals.iter().all(|&l| self.u.symbol(l).name != *name)
            }),
            _ => false,
        }
    }

    /// Reports error `msg` at `at`.
    pub fn error(&mut self, at: Span, msg: String) {
        self.host.error(at, msg);
    }

    /// Reports the errors and warnings `reports` of a macro.
    pub fn report(&mut self, reports: Reports) {
        for (level, at, msg) in reports {
            match level {
                Level::Error => self.host.error(at, msg),
                Level::Warning => self.host.warning(at, msg),
            }
        }
    }

    /// Reports the warnings `warnings` of a macro that threw `StopMacroExpansion`, and an error at
    /// `at` if none has been reported.
    pub fn aborted(&mut self, warnings: Reports, at: Span) {
        self.report(warnings);
        if !self.host.has_errors() {
            self.error(at, ABORTED.to_owned());
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
