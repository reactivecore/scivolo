// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The given and implicit terms the code being expanded defines and the imports it is in the
//! scope of, in whose scope a search runs.

use super::{Inlining, Unsupported};
use crate::ctx::Ctx;
use crate::tpd::{MatchKind, TNodeId, Tpd, Tree as T};
use crate::typer::stability::is_stable_path;
use rustc_hash::FxHashMap;
use tasty::tags::{GIVEN, IMPLICIT, MACRO};
use tasty::{SymbolId, Universe};

/// A scope inside the code being expanded: the implicits a block's statements define, or an
/// import statement.
#[derive(Clone)]
pub enum Scope {
    Implicits(Vec<SymbolId>),
    Import(TNodeId),
}

impl Inlining<'_> {
    /// Import statement `imp`, owned by `owner`, its path retyped; the statements after it are in
    /// its scope (dotty's `typedStats` of an `Import`).
    pub fn enter_import(
        &mut self,
        imp: TNodeId,
        args: &FxHashMap<SymbolId, TNodeId>,
        owner: SymbolId,
    ) -> Result<TNodeId, Unsupported> {
        let T::Import { expr, selectors, export } = *self.tpd.tree(imp) else { return Ok(imp) };
        let expr = self.retype(expr, args, owner)?;
        let stable = self.tpd.get(expr).tpe.is_some_and(|t| is_stable_path(self.u, t));
        if !stable {
            return Err((self.tpd.span(imp), "inline import of a path that is not stable"));
        }
        self.tpd.get_mut(imp).tree = T::Import { expr, selectors, export };
        self.scopes.push(Scope::Import(imp));
        Ok(imp)
    }

    /// `ctx` inside the imports among `scopes`.
    pub fn within_imports(&mut self, ctx: Ctx) -> Ctx {
        let imports: Vec<Scope> =
            self.scopes.iter().filter(|s| matches!(s, Scope::Import(_))).cloned().collect();
        imports.into_iter().fold(ctx, |ctx, scope| self.within_scope(ctx, scope))
    }

    /// `ctx` inside `scope`.
    pub fn within_scope(&mut self, ctx: Ctx, scope: Scope) -> Ctx {
        let imp = match scope {
            Scope::Implicits(defs) => {
                let locals = defs.into_iter().map(|s| (self.u.symbol(s).name.clone(), s)).collect();
                return self.host.within(ctx, locals);
            }
            Scope::Import(imp) => imp,
        };
        let T::Import { expr, selectors, .. } = *self.tpd.tree(imp) else { return ctx };
        let name = |i: &Self, t: TNodeId| match i.tpd.tree(t) {
            T::Ident { name } => i.names.text(name.name).to_owned(),
            _ => String::new(),
        };
        let selectors = (self.tpd.items(selectors).iter())
            .filter_map(|&s| match *self.tpd.tree(s) {
                T::ImportSelector { imported, renamed, bound } => {
                    let imported = name(self, imported);
                    let renamed = renamed.map_or_else(|| imported.clone(), |r| name(self, r));
                    Some((imported, renamed, bound))
                }
                _ => None,
            })
            .collect();
        let qual = self.tpd.tpe(expr);
        self.host.within_import(ctx, (qual, expr), selectors)
    }

    /// Whether the body of inline method definition `def` defines a given or implicit term other
    /// than by the pattern of a `summonFrom` case; a macro's body, which its expansion replaces,
    /// does not.
    pub fn defines_implicit(&self, def: TNodeId) -> bool {
        let T::DefDef { rhs: Some(rhs), .. } = *self.tpd.tree(def) else { return false };
        if self.tpd.sym(def).is_some_and(|s| self.u.symbol(s).flags.has(MACRO)) {
            return false;
        }
        self.has_implicit_def(rhs)
    }

    /// Whether tree `t` defines a given or implicit term that `scopes` does not track (a block
    /// statement, a case's pattern) and a search in it may see: not a method's parameter (dotty's
    /// `InlineTyper.index`) nor in an inline match's cases, which count once selected.
    pub fn has_implicit_def(&self, t: TNodeId) -> bool {
        self.is_implicit_def(t) || self.defines_inside(t)
    }

    /// Whether a part of `t` other than its parameters or pattern defines a term that
    /// `has_implicit_def` counts.
    fn defines_inside(&self, t: TNodeId) -> bool {
        let has = |t: Option<TNodeId>| t.is_some_and(|t| self.has_implicit_def(t));
        match *self.tpd.tree(t) {
            T::Match { selector, kind: MatchKind::Inline, .. } => has(selector),
            T::DefDef { rhs, .. } => has(rhs),
            T::CaseDef { guard, body, .. } => has(guard) || has(Some(body)),
            T::Block { stats, expr } => {
                let stat = |&s: &TNodeId| {
                    if self.is_implicit_def(s) { self.defines_inside(s) } else { has(Some(s)) }
                };
                self.tpd.items(stats).iter().any(stat) || has(Some(expr))
            }
            _ => self.tpd.children(t).into_iter().any(|c| self.has_implicit_def(c)),
        }
    }

    /// Adds the scope of the implicits that the statements of block `t` define, or the pattern
    /// of case `t` binds, to `scopes` if there are any; the number of scopes before (dotty's
    /// `InlineTyper.index`, `Typer.indexPattern`).
    pub fn enter_implicits(&mut self, t: TNodeId) -> usize {
        let mark = self.scopes.len();
        let mut defined = Vec::new();
        match *self.tpd.tree(t) {
            T::Block { stats, .. } => {
                let defs = self.tpd.items(stats).iter().filter(|&&s| self.is_implicit_def(s));
                defined.extend(defs.filter_map(|&s| self.tpd.sym(s)));
            }
            T::CaseDef { pat, .. } => self.implicit_defs(pat, &mut defined),
            _ => {}
        }
        if !defined.is_empty() {
            self.scopes.push(Scope::Implicits(defined));
        }
        mark
    }

    pub fn is_implicit_def(&self, t: TNodeId) -> bool {
        is_implicit_def(self.tpd, self.u, t)
    }

    /// Adds the given and implicit terms `t` defines to `found`.
    pub fn implicit_defs(&self, t: TNodeId, found: &mut Vec<SymbolId>) {
        implicit_defs(self.tpd, self.u, t, found);
    }
}

/// Adds the given and implicit terms `t` defines to `found`.
fn implicit_defs(tpd: &Tpd, u: &Universe, t: TNodeId, found: &mut Vec<SymbolId>) {
    found.extend(tpd.sym(t).filter(|_| is_implicit_def(tpd, u, t)));
    for c in tpd.children(t) {
        implicit_defs(tpd, u, c, found);
    }
}

/// Whether `t` defines a given or implicit value or method, or binds a given in a pattern.
pub fn is_implicit_def(tpd: &Tpd, u: &Universe, t: TNodeId) -> bool {
    let def = matches!(tpd.tree(t), T::ValDef { .. } | T::DefDef { .. } | T::Bind { .. });
    def && tpd.sym(t).is_some_and(|s| {
        let flags = u.symbol(s).flags;
        flags.has(GIVEN) || flags.has(IMPLICIT)
    })
}

/// Determines the scopes of the given and implicit terms `t` defines in whose scope `call` is,
/// outermost first: `t` if it defines one, then for each tree containing `call` the terms its
/// children define, or a case's pattern binds; `None` if `t` does not contain `call`.
pub fn implicit_scopes_at(
    tpd: &Tpd,
    u: &Universe,
    (t, call): (TNodeId, TNodeId),
) -> Option<Vec<Vec<SymbolId>>> {
    let mut scopes = Vec::new();
    if !scopes_within(tpd, u, (t, call), &mut scopes) {
        return None;
    }
    scopes.extend(tpd.sym(t).filter(|_| is_implicit_def(tpd, u, t)).map(|s| vec![s]));
    scopes.reverse();
    Some(scopes)
}

/// Adds the scopes of `implicit_scopes_at` inside `t` to `found`, innermost first; whether `t`
/// contains `call`.
fn scopes_within(
    tpd: &Tpd,
    u: &Universe,
    (t, call): (TNodeId, TNodeId),
    found: &mut Vec<Vec<SymbolId>>,
) -> bool {
    if t == call {
        return true;
    }
    let children = tpd.children(t);
    if !children.iter().any(|&c| scopes_within(tpd, u, (c, call), found)) {
        return false;
    }
    let mut defined = Vec::new();
    match *tpd.tree(t) {
        T::CaseDef { pat, .. } => implicit_defs(tpd, u, pat, &mut defined),
        _ => defined
            .extend(children.into_iter().filter(|&c| is_implicit_def(tpd, u, c)).filter_map(|c| tpd.sym(c))),
    }
    if !defined.is_empty() {
        found.push(defined);
    }
    true
}
