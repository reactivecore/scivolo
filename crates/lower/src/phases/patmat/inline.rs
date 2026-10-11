//! Variables of plans used once inlined and code after plans that cannot fall through dropped
//! (dotty's `inlineVars`).

use super::Translator;
use super::plan::{Plan, PlanId, set_on_success};
use crate::phases::walk::{children, subtrees};
use crate::symbols::has;
use rustc_hash::FxHashMap;
use tasty::tags::{LAZY, MUTABLE};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

impl Translator<'_, '_> {
    /// `p` with the variables used once and bound to pure values inlined, and the code after
    /// plans that cannot fall through dropped.
    pub fn inline_vars(&mut self, p: PlanId) -> PlanId {
        let mut counts = FxHashMap::default();
        self.count_refs(p, &mut counts);
        let Plan::Let { sym: top, .. } = *self.plan(p) else { unreachable!("a match's binding") };
        let drop = |t: &Self, s: SymbolId| {
            s != top
                && t.is_generated(s)
                && counts.get(&s).copied().unwrap_or(0) <= 1
                && t.initializer.get(&s).is_some_and(|&rhs| t.is_pure(rhs))
        };
        self.inline(p, &drop)
    }

    fn inline(&mut self, p: PlanId, drop: &dyn Fn(&Self, SymbolId) -> bool) -> PlanId {
        match *self.plan(p) {
            Plan::Test { scrutinee, on_success, .. } => {
                self.inline_tree(scrutinee, drop);
                let on_success = self.inline(on_success, drop);
                set_on_success(self.plan_mut(p), on_success);
                p
            }
            Plan::Let { sym, body } if drop(self, sym) => self.inline(body, drop),
            Plan::Let { sym, body } => {
                self.inline_tree(self.initializer[&sym], drop);
                let body = self.inline(body, drop);
                *self.plan_mut(p) = Plan::Let { sym, body };
                p
            }
            Plan::Labeled { sym, expr } => {
                let expr = self.inline(expr, drop);
                *self.plan_mut(p) = Plan::Labeled { sym, expr };
                p
            }
            Plan::Return { .. } | Plan::Result { .. } => p,
            Plan::Seq { head, tail } => {
                let head = self.inline(head, drop);
                if !self.can_fall_through(head) {
                    return head;
                }
                let tail = self.inline(tail, drop);
                *self.plan_mut(p) = Plan::Seq { head, tail };
                p
            }
        }
    }

    /// Replaces the references in `t` to the variables to drop by their initializers, an
    /// initializer without span at the reference's.
    fn inline_tree(&mut self, t: TNodeId, drop: &dyn Fn(&Self, SymbolId) -> bool) {
        let mut todo = vec![t];
        while let Some(t) = todo.pop() {
            if let (Tree::Ident { .. }, Some(s)) = (self.l.tpd.tree(t), self.l.tpd.sym(t))
                && drop(self, s)
            {
                let span = self.l.tpd.span(t);
                self.l.copy_node(t, self.initializer[&s]);
                if !self.l.tpd.span(t).exists() {
                    self.l.with_span(t, span);
                }
            }
            todo.extend(children(self.l.tpd, t));
        }
    }

    /// Counts the references to each generated variable in the code of `p` that may run.
    fn count_refs(&self, p: PlanId, counts: &mut FxHashMap<SymbolId, usize>) {
        let count = |t: TNodeId, counts: &mut FxHashMap<SymbolId, usize>| {
            for node in subtrees(self.l.tpd, t) {
                if let (Tree::Ident { .. }, Some(s)) = (self.l.tpd.tree(node), self.l.tpd.sym(node))
                    && self.is_generated(s)
                {
                    *counts.entry(s).or_insert(0) += 1;
                }
            }
        };
        match *self.plan(p) {
            Plan::Test { scrutinee, on_success, .. } => {
                count(scrutinee, counts);
                self.count_refs(on_success, counts);
            }
            Plan::Let { sym, body } => {
                self.count_refs(body, counts);
                if counts.get(&sym).is_some_and(|&n| n != 0) || !self.is_generated(sym) {
                    count(self.initializer[&sym], counts);
                }
            }
            Plan::Labeled { expr, .. } => self.count_refs(expr, counts),
            Plan::Seq { head, tail } => {
                self.count_refs(head, counts);
                if self.can_fall_through(head) {
                    self.count_refs(tail, counts);
                }
            }
            Plan::Return { .. } | Plan::Result { .. } => {}
        }
    }

    /// Whether evaluating `t` has no effect (dotty's `isPureExpr`, for the trees plans bind).
    fn is_pure(&self, t: TNodeId) -> bool {
        match *self.l.tpd.tree(t) {
            Tree::Literal { .. } | Tree::ClassLiteral { .. } | Tree::This { .. } => true,
            Tree::Ident { .. } => self.l.tpd.sym(t).is_none_or(|s| {
                self.l.u.symbol(s).kind != Kind::Def && !has(self.l.u, s, MUTABLE) && !has(self.l.u, s, LAZY)
            }),
            Tree::Select { qualifier, .. } => {
                let stable = self.l.tpd.sym(t).is_some_and(|s| {
                    matches!(self.l.u.symbol(s).kind, Kind::Val | Kind::Param)
                        && !has(self.l.u, s, MUTABLE)
                        && !has(self.l.u, s, LAZY)
                });
                stable && self.is_pure(qualifier)
            }
            Tree::TypeApply { fun, .. } if self.l.tpd.sym(fun) == Some(self.l.defs.as_instance_of) => {
                matches!(*self.l.tpd.tree(fun), Tree::Select { qualifier, .. } if self.is_pure(qualifier))
            }
            _ => false,
        }
    }
}
