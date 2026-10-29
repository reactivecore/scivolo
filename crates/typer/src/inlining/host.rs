//! What an expansion asks of the typer it runs in: the context of a call, implicit arguments,
//! errors (dotty's `Inliner` working in the context of the call).

use super::{AnnotationCall, Expanded, Inlining, MacroCall, MacroError, Unsupported};
use crate::ctx::Ctx;
use crate::tpd::{MatchKind, TNodeId, Tpd, Tree as T};
use syntax::ast::Span;
use syntax::names::{FreshNames, Names};
use tasty::tags::{GIVEN, IMPLICIT};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

/// The trees and tables an expansion works on, lent back to the typer for a search.
pub type Lent<'l> = (&'l mut Universe, &'l mut Tpd, &'l mut Names, &'l mut FreshNames);

pub trait Host {
    /// The context of inline call `call`, outside an inline method, as the typer recorded it.
    fn call_ctx(&self, call: TNodeId) -> Option<Ctx>;
    /// `ctx` inside a scope of `locals` by name.
    fn within(&mut self, ctx: Ctx, locals: Vec<(String, SymbolId)>) -> Ctx;
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
    /// How many errors have been reported, and whether there are more than `mark`, which it
    /// takes back (for `typeChecks`).
    fn error_count(&self) -> usize;
    fn errors_since(&mut self, mark: usize) -> bool;
    /// Statements `code` parsed and typed as a block in `ctx`, `None` if that reports an error
    /// (dotty's `Inlines.Intrinsics.compileForErrors` up to the typer).
    fn typed_code(&mut self, lent: Lent<'_>, code: &str, ctx: Ctx) -> Option<TNodeId>;
    /// The typed definition of inline method `meth` of the unit, `Ok(None)` if it has none.
    fn inline_def(&mut self, lent: Lent<'_>, meth: SymbolId) -> Result<Option<TNodeId>, &'static str>;
    /// What macro call `call` expands to (dotty's `Splicer.splice`).
    fn expand_macro(&mut self, lent: Lent<'_>, call: &MacroCall) -> Result<Expanded, MacroError>;
    /// The typed annotations on `s`, a symbol of the unit, in source order.
    fn annotations(&self, s: SymbolId) -> Vec<TNodeId>;
    /// The definitions macro annotation call `call` returns (dotty's `MacroAnnotations.callMacro`).
    fn expand_annotation(&mut self, lent: Lent<'_>, call: &AnnotationCall) -> Result<Expanded, MacroError>;
}

impl Inlining<'_> {
    /// An implicit argument of type `pt` at `at` in the context of the outermost call; `Ok(None)`
    /// if there is none. Refused where implicits the expansion defines may take part.
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
        let mut blocks = self.block_implicits.concat();
        blocks.retain(|b| !self.case_givens.contains(b));
        match fitting[..] {
            [] => {}
            [_] if self.may_fit(&blocks, pt) => {
                return Err((
                    at,
                    "implicit search fitting a given of a summonFrom case and a block implicit",
                ));
            }
            [g] => {
                let (name, tpe) = (self.term_name(g), self.u.intern_type(Type::Sym(None, g)));
                return Ok(Some(self.tpd.node(T::Ident { name }, at.to_synthetic(), Some(tpe), Some(g))));
            }
            _ => return Err((at, "implicit search fitting several givens of summonFrom cases")),
        }
        let mut ctx = ctx;
        for scope in self.block_implicits.clone() {
            let locals = scope.into_iter().map(|s| (self.u.symbol(s).name.clone(), s)).collect();
            ctx = self.host.within(ctx, locals);
        }
        let lent = (&mut *self.u, &mut *self.tpd, &mut *self.names, &mut *self.fresh);
        let found = self.host.summon(lent, pt, at, ctx).map_err(|what| (at, what))?;
        let locals = [&self.outer_implicits[..], &self.case_givens[..]].concat();
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

    /// Adds the scope of the implicits the statements of `t` define to `block_implicits` if it is
    /// a block that defines any; the number of scopes before.
    pub fn enter_block_implicits(&mut self, t: TNodeId) -> usize {
        let mark = self.block_implicits.len();
        if let T::Block { stats, .. } = *self.tpd.tree(t) {
            let stats = self.tpd.items(stats).to_vec();
            let defined: Vec<SymbolId> = stats
                .into_iter()
                .filter(|&s| self.is_implicit_def(s))
                .filter_map(|s| self.tpd.sym(s))
                .collect();
            if !defined.is_empty() {
                self.block_implicits.push(defined);
            }
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

    /// Adds the given and implicit terms `t` defines to `found`.
    pub fn implicit_defs(&self, t: TNodeId, found: &mut Vec<SymbolId>) {
        if self.is_implicit_def(t)
            && let Some(s) = self.tpd.sym(t)
        {
            found.push(s);
        }
        for c in self.tpd.children(t) {
            self.implicit_defs(c, found);
        }
    }

    /// Adds the given and implicit terms `t` defines in whose scope `call` is to `found`: those
    /// defined beside a tree containing it, or defining it; whether `t` contains `call`.
    pub fn implicit_defs_at(&self, t: TNodeId, call: TNodeId, found: &mut Vec<SymbolId>) -> bool {
        if t == call {
            return true;
        }
        let children = self.tpd.children(t);
        let Some(i) = children.iter().position(|&c| self.implicit_defs_at(c, call, found)) else {
            return false;
        };
        let beside = children.iter().enumerate().filter(|&(j, &c)| j != i && self.is_implicit_def(c));
        found.extend(beside.filter_map(|(_, &c)| self.tpd.sym(c)));
        found.extend(self.tpd.sym(t).filter(|_| self.is_implicit_def(t)));
        true
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
