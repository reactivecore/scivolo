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

    /// The implicits the blocks in `scopes` define.
    pub fn block_defs(&self) -> Vec<SymbolId> {
        let defs = self.scopes.iter().flat_map(|s| match s {
            Scope::Implicits(defs) => defs.as_slice(),
            Scope::Import(_) => &[],
        });
        defs.copied().collect()
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

    /// Whether tree `t` defines a given or implicit term other than by a block statement, which
    /// `scopes` tracks, outside the cases of inline matches, which count once their
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

    /// Adds the scope of the implicits the statements of `t` define to `scopes` if it is a block
    /// that defines any; the number of scopes before.
    pub fn enter_block_implicits(&mut self, t: TNodeId) -> usize {
        let mark = self.scopes.len();
        if let T::Block { stats, .. } = *self.tpd.tree(t) {
            let stats = self.tpd.items(stats).to_vec();
            let defined: Vec<SymbolId> = stats
                .into_iter()
                .filter(|&s| self.is_implicit_def(s))
                .filter_map(|s| self.tpd.sym(s))
                .collect();
            if !defined.is_empty() {
                self.scopes.push(Scope::Implicits(defined));
            }
        }
        mark
    }

    pub fn is_implicit_def(&self, t: TNodeId) -> bool {
        is_implicit_def(self.tpd, self.u, t)
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

    pub fn implicit_defs_at(&self, t: TNodeId, call: TNodeId, found: &mut Vec<SymbolId>) -> bool {
        implicit_defs_at(self.tpd, self.u, (t, call), found)
    }
}

/// Whether `t` defines a given or implicit value or method.
pub fn is_implicit_def(tpd: &Tpd, u: &Universe, t: TNodeId) -> bool {
    let def = matches!(tpd.tree(t), T::ValDef { .. } | T::DefDef { .. });
    def && tpd.sym(t).is_some_and(|s| {
        let flags = u.symbol(s).flags;
        flags.has(GIVEN) || flags.has(IMPLICIT)
    })
}

/// Adds the given and implicit terms `t` defines in whose scope `call` is to `found`: those
/// defined beside a tree containing it, or defining it; whether `t` contains `call`.
pub fn implicit_defs_at(
    tpd: &Tpd,
    u: &Universe,
    (t, call): (TNodeId, TNodeId),
    found: &mut Vec<SymbolId>,
) -> bool {
    if t == call {
        return true;
    }
    let children = tpd.children(t);
    let Some(i) = children.iter().position(|&c| implicit_defs_at(tpd, u, (c, call), found)) else {
        return false;
    };
    let beside = children.iter().enumerate().filter(|&(j, &c)| j != i && is_implicit_def(tpd, u, c));
    found.extend(beside.filter_map(|(_, &c)| tpd.sym(c)));
    found.extend(tpd.sym(t).filter(|_| is_implicit_def(tpd, u, t)));
    true
}
