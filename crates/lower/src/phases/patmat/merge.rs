//! Identical consecutive tests and bindings of plans merged (dotty's `mergeTests`).

use super::Translator;
use super::plan::{Plan, PlanId, Test, set_on_success};
use crate::phases::walk::{same, subtrees};
use tasty::{SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

impl Translator<'_, '_> {
    /// `p` with identical consecutive tests and bindings merged.
    pub fn merge_tests(&mut self, p: PlanId) -> PlanId {
        match *self.plan(p) {
            Plan::Test { on_success, .. } => {
                let merged = self.merge_tests(on_success);
                set_on_success(self.plan_mut(p), merged);
            }
            Plan::Let { sym, body } => {
                let body = self.merge_tests(body);
                *self.plan_mut(p) = Plan::Let { sym, body };
            }
            Plan::Labeled { sym, expr } => {
                let expr = self.merge_tests(expr);
                *self.plan_mut(p) = Plan::Labeled { sym, expr };
            }
            Plan::Return { .. } | Plan::Result { .. } => {}
            Plan::Seq { head, tail } => {
                let (head, tail) = (self.merge_tests(head), self.merge_tests(tail));
                *self.plan_mut(p) = Plan::Seq { head, tail };
                return match *self.plan(tail) {
                    Plan::Seq { head: next, tail: rest } => match self.try_merge(head, next) {
                        Some(merged) => {
                            let head = self.merge_tests(merged);
                            self.add(Plan::Seq { head, tail: rest })
                        }
                        None => p,
                    },
                    _ => self.try_merge(head, tail).map_or(p, |merged| self.merge_tests(merged)),
                };
            }
        }
        p
    }

    /// `p1` followed by `p2` as one plan, if they start with the same test or binding.
    fn try_merge(&mut self, p1: PlanId, p2: PlanId) -> Option<PlanId> {
        let (s1, s2) = (self.skip_user_lets(p1), self.skip_user_lets(p2));
        let same_tests = self.same_test(s1, s2);
        match (self.plan(s1), self.plan(s2)) {
            (&Plan::Test { on_success: ons1, .. }, &Plan::Test { on_success: ons2, .. }) if same_tests => {
                let rest = self.transfer_user_lets(p2, ons2);
                let merged = self.add(Plan::Seq { head: ons1, tail: rest });
                set_on_success(self.plan_mut(s1), merged);
                Some(p1)
            }
            (&Plan::Let { sym: sym1, body: body1 }, &Plan::Let { sym: sym2, body: body2 })
                if same(self.l.tpd, self.initializer[&sym1], self.initializer[&sym2]) =>
            {
                self.substitute(body2, sym2, sym1);
                let rest = self.transfer_user_lets(p2, body2);
                let body = self.add(Plan::Seq { head: body1, tail: rest });
                *self.plan_mut(s1) = Plan::Let { sym: sym1, body };
                Some(p1)
            }
            _ => None,
        }
    }

    fn same_test(&mut self, a: PlanId, b: PlanId) -> bool {
        let (&Plan::Test { test: t1, scrutinee: s1, .. }, &Plan::Test { test: t2, scrutinee: s2, .. }) =
            (self.plan(a), self.plan(b))
        else {
            return false;
        };
        let tests = match (t1, t2) {
            (Test::Equal(a), Test::Equal(b)) => same(self.l.tpd, a, b),
            (Test::Guard, Test::Guard)
            | (Test::NonNull, Test::NonNull)
            | (Test::NonEmpty, Test::NonEmpty) => true,
            (Test::Type(a), Test::Type(b)) => {
                let (a, b) = (self.l.tpd.tpe(a), self.l.tpd.tpe(b));
                self.l.u.dealias(a) == self.l.u.dealias(b) || self.same_class(a, b)
            }
            _ => false,
        };
        tests && same(self.l.tpd, s1, s2)
    }

    /// Whether `a` and `b` both are references to the same class, through aliases.
    fn same_class(&mut self, a: TypeId, b: TypeId) -> bool {
        let mut class = |t: TypeId| {
            let t = self.l.u.dealias(t);
            let plain = matches!(self.l.u.types().get(t), Type::Sym(..) | Type::TypeRef(..));
            plain.then(|| self.l.u.class_symbol(t)).flatten()
        };
        let a = class(a);
        a.is_some() && a == class(b)
    }

    /// The plan after the bindings of user variables `p` starts with.
    fn skip_user_lets(&self, p: PlanId) -> PlanId {
        match *self.plan(p) {
            Plan::Let { sym, body } if !self.is_generated(sym) => self.skip_user_lets(body),
            _ => p,
        }
    }

    /// The bindings of user variables `original` starts with, around `new`.
    fn transfer_user_lets(&mut self, original: PlanId, new: PlanId) -> PlanId {
        match *self.plan(original) {
            Plan::Let { sym, body } if !self.is_generated(sym) => {
                let body = self.transfer_user_lets(body, new);
                *self.plan_mut(original) = Plan::Let { sym, body };
                original
            }
            _ => new,
        }
    }

    /// Makes the tests and initializers in `p` refer to `to` instead of `from`.
    fn substitute(&mut self, p: PlanId, from: SymbolId, to: SymbolId) {
        for t in self.plan_trees(p) {
            for node in subtrees(self.l.tpd, t) {
                if matches!(self.l.tpd.tree(node), Tree::Ident { .. }) && self.l.tpd.sym(node) == Some(from) {
                    let ident = self.l.ident(to, self.l.tpd.span(node));
                    self.l.copy_node(node, ident);
                }
            }
        }
    }

    /// The tested trees and the initializers of the bindings in `p`.
    fn plan_trees(&self, p: PlanId) -> Vec<TNodeId> {
        match *self.plan(p) {
            Plan::Test { scrutinee, on_success, .. } => {
                std::iter::once(scrutinee).chain(self.plan_trees(on_success)).collect()
            }
            Plan::Let { sym, body } => {
                std::iter::once(self.initializer[&sym]).chain(self.plan_trees(body)).collect()
            }
            Plan::Labeled { expr, .. } => self.plan_trees(expr),
            Plan::Seq { head, tail } => {
                let mut out = self.plan_trees(head);
                out.extend(self.plan_trees(tail));
                out
            }
            Plan::Return { .. } | Plan::Result { .. } => Vec::new(),
        }
    }
}
