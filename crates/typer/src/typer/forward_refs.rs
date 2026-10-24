// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ForwardDepChecks.scala
//! References in a block to a definition of the block that a value definition between them
//! precedes: an error (dotty's `ForwardDepChecks`, without the self constructor call check).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::SymbolId;
use tasty::tags::LAZY;

/// The definitions of one block by index, and its greatest index referred to so far.
struct Level {
    index: FxHashMap<SymbolId, usize>,
    max: Option<(usize, Span, SymbolId)>,
}

impl Typer<'_> {
    /// Reports each forward reference in unit `root` that extends over a value definition.
    pub fn check_forward_refs(&mut self, root: TNodeId) {
        let mut errors = Vec::new();
        self.forward_refs_in(root, &mut Vec::new(), &mut errors);
        for (span, msg) in errors {
            self.error_at(span, msg);
        }
    }

    fn forward_refs_in(&mut self, t: TNodeId, levels: &mut Vec<Level>, errors: &mut Vec<(Span, String)>) {
        let sym = self.tpd.sym(t);
        match *self.tpd.tree(t) {
            T::Block { stats, expr } => {
                let stats = self.tpd.items(stats).to_vec();
                let defs = stats.iter().enumerate().filter_map(|(i, &s)| self.member_def(s).map(|d| (d, i)));
                levels.push(Level { index: defs.collect(), max: None });
                for s in stats.into_iter().chain([expr]) {
                    self.forward_refs_in(s, levels, errors);
                }
                levels.pop();
                return;
            }
            T::Inlined { bindings, expansion, .. } => {
                for b in self.tpd.items(bindings).to_vec().into_iter().chain([expansion]) {
                    self.forward_refs_in(b, levels, errors);
                }
                return;
            }
            T::SingletonTypeTree { .. } => return,
            T::Ident { .. } => {
                let term = sym.filter(|&s| self.u.symbol(s).kind.is_term());
                term.into_iter().for_each(|s| self.enter_reference(s, t, levels));
            }
            T::New { tpt } => {
                let class = self.tpd.get(tpt).tpe.and_then(|tpe| self.u.class_symbol(tpe));
                class.into_iter().for_each(|c| self.enter_reference(c, t, levels));
            }
            _ => {}
        }
        for c in self.tpd.children(t) {
            self.forward_refs_in(c, levels, errors);
        }
        if let (T::ValDef { .. }, Some(s)) = (self.tpd.tree(t), sym)
            && !self.u.symbol(s).flags.has(LAZY)
            && !self.u.is_module(s)
            && let Some(level) = levels.iter().rev().find(|l| l.index.contains_key(&s))
            && let Some((max, span, referred)) = level.max
            && level.index[&s] <= max
        {
            let (referred, value) = (&self.u.symbol(referred).name, &self.u.symbol(s).name);
            errors.push((
                span,
                format!("forward reference to {referred} extends over the definition of {value}"),
            ));
        }
    }

    /// The symbol `stat` defines if it is a member definition.
    fn member_def(&self, stat: TNodeId) -> Option<SymbolId> {
        matches!(self.tpd.tree(stat), T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. })
            .then(|| self.tpd.sym(stat))
            .flatten()
    }

    /// Records the reference `t` to `s` in the innermost block defining `s`.
    fn enter_reference(&self, s: SymbolId, t: TNodeId, levels: &mut [Level]) {
        let Some(level) = levels.iter_mut().rev().find(|l| l.index.contains_key(&s)) else { return };
        let idx = level.index[&s];
        if level.max.is_none_or(|(max, ..)| max < idx) {
            level.max = Some((idx, self.tpd.span(t), s));
        }
    }
}
