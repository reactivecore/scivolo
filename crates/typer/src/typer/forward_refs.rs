// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ForwardDepChecks.scala
//! References in a block to a definition of the block that a value definition between them
//! precedes: an error (dotty's `ForwardDepChecks`, without the self constructor call check).

use super::Typer;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::LAZY;
use tasty::{SymbolId, Type, Universe};

/// The definitions of one block by index, and its greatest index referred to so far.
struct Level {
    index: FxHashMap<SymbolId, usize>,
    max: Option<(usize, Span, SymbolId)>,
}

/// One check of a typed unit for references in a block that reach over a value definition to a
/// later definition, as `x` in `{ def f = x; val y = f; val x = 1 }`.
pub struct ForwardRefs<'a> {
    pub tpd: &'a Tpd,
    /// The blocks around the tree being checked, innermost last.
    levels: Vec<Level>,
    /// The errors found, with dotty's messages.
    pub errors: Vec<(Span, String)>,
}

impl Typer<'_> {
    /// Reports each forward reference in unit `root` that extends over a value definition.
    pub fn check_forward_refs(&mut self, root: TNodeId) {
        let mut check = ForwardRefs::new(&self.tpd);
        check.refs_in(self.u, root);
        for (span, msg) in check.errors {
            self.error_at(span, msg);
        }
    }
}

impl<'a> ForwardRefs<'a> {
    /// Starts a check of the trees of `tpd`.
    pub fn new(tpd: &'a Tpd) -> Self {
        ForwardRefs { tpd, levels: Vec::new(), errors: Vec::new() }
    }

    /// Checks tree `t` and its subtrees.
    pub fn refs_in(&mut self, u: &mut Universe, t: TNodeId) {
        let tpd = self.tpd;
        let sym = tpd.sym(t);
        match *tpd.tree(t) {
            T::Block { stats, expr } => {
                let stats = tpd.items(stats);
                let defs = stats.iter().enumerate().filter_map(|(i, &s)| self.member_def(s).map(|d| (d, i)));
                self.levels.push(Level { index: defs.collect(), max: None });
                for &s in stats.iter().chain([&expr]) {
                    self.refs_in(u, s);
                }
                self.levels.pop();
                return;
            }
            T::Inlined { bindings, expansion, .. } => {
                for &b in tpd.items(bindings).iter().chain([&expansion]) {
                    self.refs_in(u, b);
                }
                return;
            }
            _ if tpd.is_type_tree(t) => return,
            T::Ident { .. } => sym.into_iter().for_each(|s| self.enter_reference(s, t)),
            T::New { tpt } => {
                let Some(tpe) = tpd.get(tpt).tpe else { return };
                let mut refs: Vec<SymbolId> = u.class_symbol(tpe).into_iter().collect();
                let tpe = u.dealias(tpe);
                u.map_type(tpe, &mut |u, part| {
                    if let Type::Sym(_, s) = *u.types().get(part)
                        && u.symbol(s).kind.is_term()
                    {
                        refs.push(s);
                    }
                    None
                });
                refs.into_iter().for_each(|s| self.enter_reference(s, t));
                return;
            }
            _ => {}
        }
        for c in tpd.children(t) {
            self.refs_in(u, c);
        }
        if let (T::ValDef { .. }, Some(s)) = (tpd.tree(t), sym)
            && !u.symbol(s).flags.has(LAZY)
            && !u.is_module(s)
            && let Some(level) = self.levels.iter().rev().find(|l| l.index.contains_key(&s))
            && let Some((max, span, referred)) = level.max
            && level.index[&s] <= max
        {
            let (referred, value) = (&u.symbol(referred).name, &u.symbol(s).name);
            self.errors.push((
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
    fn enter_reference(&mut self, s: SymbolId, t: TNodeId) {
        let Some(level) = self.levels.iter_mut().rev().find(|l| l.index.contains_key(&s)) else { return };
        let idx = level.index[&s];
        if level.max.is_none_or(|(max, ..)| max < idx) {
            level.max = Some((idx, self.tpd.span(t), s));
        }
    }
}
