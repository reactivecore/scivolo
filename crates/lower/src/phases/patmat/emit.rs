//! Plans emitted as trees: tests as conditionals, bindings as value definitions, labeled blocks
//! and returns from them, sequences as blocks, results as returns from the match. New trees take
//! their spans from their children, as in dotty.

use super::Translator;
use super::plan::{Plan, PlanId, Test};
use crate::definitions::type_ref;
use syntax::ast::Span;
use tasty::{SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

impl Translator<'_, '_> {
    pub fn emit(&mut self, p: PlanId) -> TNodeId {
        match *self.plan(p) {
            Plan::Test { .. } => {
                let mut tests = vec![p];
                while let Plan::Test { on_success, .. } = *self.plan(*tests.last().expect("a test"))
                    && matches!(self.plan(on_success), Plan::Test { .. })
                {
                    tests.push(on_success);
                }
                let mut cond = self.condition(tests[0]);
                for &t in &tests[1..] {
                    let next = self.condition(t);
                    cond = self.and(cond, next);
                }
                let Plan::Test { on_success, .. } = *self.plan(*tests.last().expect("a test")) else {
                    unreachable!("a test")
                };
                let thenp = self.emit(on_success);
                let elsep = self.l.unit_literal(Span::NONE);
                let unit = self.unit_type();
                self.node(Tree::If { cond, thenp, elsep, inline: false }, unit, None)
            }
            Plan::Let { sym, body } => {
                let def = self.val_def(sym);
                let body = self.emit(body);
                self.seq(&[def], body)
            }
            Plan::Labeled { sym, expr } => {
                let expr = self.emit(expr);
                self.labeled(sym, expr)
            }
            Plan::Return { label } => {
                let unit = self.l.unit_literal(Span::NONE);
                self.ret(unit, label)
            }
            Plan::Seq { head, tail } => {
                let scrutinee = match *self.plan(head) {
                    Plan::Test { scrutinee, .. } => Some(scrutinee),
                    Plan::Labeled { expr, .. } => self.alternatives_test(expr),
                    _ => None,
                };
                if let Some(switch) = scrutinee.and_then(|s| self.switch(s, p)) {
                    return switch;
                }
                let head = self.emit(head);
                let tail = self.emit(tail);
                self.seq(&[head], tail)
            }
            Plan::Result { tree } if self.is_throw(tree) => tree,
            Plan::Result { tree } => self.ret(tree, self.result_label),
        }
    }

    /// The scrutinee of the test of alternatives `expr` starts with: `l: { l2: { test ... } ... }`.
    fn alternatives_test(&self, expr: PlanId) -> Option<TNodeId> {
        let Plan::Seq { head, .. } = *self.plan(expr) else { return None };
        let Plan::Labeled { expr, .. } = *self.plan(head) else { return None };
        let Plan::Seq { head, .. } = *self.plan(expr) else { return None };
        match *self.plan(head) {
            Plan::Test { scrutinee, .. } => Some(scrutinee),
            _ => None,
        }
    }

    fn is_throw(&self, t: TNodeId) -> bool {
        let fun = match *self.l.tpd.tree(t) {
            Tree::Apply { fun, .. } => fun,
            _ => t,
        };
        self.l.tpd.sym(fun) == Some(self.l.defs.patmat.throw)
    }

    /// The condition of test `p`, at the tested pattern.
    fn condition(&mut self, p: PlanId) -> TNodeId {
        let Plan::Test { test, scrutinee, span, .. } = *self.plan(p) else { unreachable!("a test") };
        let cond = match test {
            Test::Guard => scrutinee,
            Test::Equal(tree) => self.l.equal(tree, scrutinee),
            Test::Type(tpt) => self.type_test(scrutinee, tpt),
            Test::NonNull => {
                let null = self.l.null_literal(Span::NONE);
                self.l.call(scrutinee, self.l.defs.patmat.ne, &[null])
            }
            Test::NonEmpty => self.non_empty(scrutinee),
            Test::Length { len, exact } => self.length_test(scrutinee, len, exact),
        };
        self.l.with_span(cond, span)
    }

    /// `a && b`.
    fn and(&mut self, a: TNodeId, b: TNodeId) -> TNodeId {
        self.l.call(a, self.l.defs.patmat.and, &[b])
    }

    /// The definition of variable `sym` with its initializer, at the variable.
    fn val_def(&mut self, sym: SymbolId) -> TNodeId {
        let rhs = self.initializer[&sym];
        let name = self.l.build().term_name(sym);
        let info = self.l.u.info(sym);
        let tpt = self.l.type_tree(info, Span::NONE);
        let def = self.node(Tree::ValDef { name, tpt, rhs: Some(rhs) }, info, Some(sym));
        self.l.with_span(def, self.spans[&sym])
    }

    /// `stats` before `expr`, joined with the statements of block `expr` (dotty's `seq`).
    pub fn seq(&mut self, stats: &[TNodeId], expr: TNodeId) -> TNodeId {
        match *self.l.tpd.tree(expr) {
            Tree::Block { stats: inner, expr: last }
                if !matches!(self.l.tpd.tree(last), Tree::Closure { .. }) =>
            {
                let all: Vec<TNodeId> = stats.iter().chain(self.l.tpd.items(inner)).copied().collect();
                let span = self.l.tpd.span(expr);
                let block = self.l.block(&all, last, Span::NONE);
                self.l.positioned(block);
                self.l.tpd.get_mut(block).span = span;
                block
            }
            _ => {
                let block = self.l.block(stats, expr, Span::NONE);
                self.l.positioned(block)
            }
        }
    }

    pub fn labeled(&mut self, label: SymbolId, expr: TNodeId) -> TNodeId {
        let tpe = self.l.u.info(label);
        self.node(Tree::Labeled { expr }, tpe, Some(label))
    }

    /// `return[label] expr`.
    fn ret(&mut self, expr: TNodeId, label: SymbolId) -> TNodeId {
        let from = self.l.ident(label, Span::NONE);
        let nothing = self.nothing_type();
        self.node(Tree::Return { expr, from }, nothing, None)
    }

    /// A new tree at the span of its children.
    pub fn node(&mut self, tree: Tree, tpe: TypeId, sym: Option<SymbolId>) -> TNodeId {
        let t = self.l.tpd.node(tree, Span::NONE, Some(tpe), sym);
        self.l.positioned(t)
    }

    pub fn unit_type(&mut self) -> TypeId {
        type_ref(self.l.u, self.l.defs.patmat.unit)
    }

    pub fn nothing_type(&mut self) -> TypeId {
        let nothing = self.l.u.builtins().nothing;
        type_ref(self.l.u, nothing)
    }
}
