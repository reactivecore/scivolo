//! The given and implicit terms the code being expanded defines, in whose scope a search runs.

use super::Inlining;
use crate::tpd::{MatchKind, TNodeId, Tree as T};
use tasty::SymbolId;
use tasty::tags::{GIVEN, IMPLICIT, MACRO};

impl Inlining<'_> {
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
}
