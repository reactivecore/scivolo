//! Plans emitted as trees: tests as conditionals, bindings as value definitions, labeled blocks
//! and returns from them, sequences as blocks, results as returns from the match. New trees take
//! their spans from their children, as in dotty.

use super::Translator;
use super::plan::{Plan, PlanId, Test};
use crate::definitions::type_ref;
use syntax::ast::Span;
use tasty::{SymbolId, Type, TypeId};
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
                self.node(Tree::If { cond, thenp, elsep }, unit, None)
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
            Test::Equal(tree) => self.equal(tree, scrutinee),
        };
        self.l.with_span(cond, span)
    }

    /// `lhs == rhs`, with the `==` of `lhs`'s class taking `rhs`'s class if any, else `Any`'s.
    fn equal(&mut self, lhs: TNodeId, rhs: TNodeId) -> TNodeId {
        let (lhs_tpe, rhs_tpe) = (self.l.u.widen(self.l.tpd.tpe(lhs)), self.l.u.widen(self.l.tpd.tpe(rhs)));
        let class = self.l.u.class_symbol(lhs_tpe);
        let arg = self.l.u.class_symbol(rhs_tpe);
        let eq = class
            .and_then(|c| self.l.u.decl(c, "==").ok())
            .and_then(|eqs| eqs.into_iter().find(|&m| self.single_param_class(m) == arg))
            .unwrap_or(self.l.defs.patmat.any_eq);
        self.call(lhs, eq, &[rhs])
    }

    fn single_param_class(&mut self, m: SymbolId) -> Option<SymbolId> {
        let info = self.l.u.info(m);
        let Type::Method { params, .. } = self.l.u.types().get(info).clone() else { return None };
        let &[(_, p)] = params.as_slice() else { return None };
        let p = self.l.u.widen(p);
        self.l.u.class_symbol(p)
    }

    /// `a && b`.
    fn and(&mut self, a: TNodeId, b: TNodeId) -> TNodeId {
        self.call(a, self.l.defs.patmat.and, &[b])
    }

    /// `qual.m(args)`, of a method `m` with one parameter list.
    pub fn call(&mut self, qual: TNodeId, m: SymbolId, args: &[TNodeId]) -> TNodeId {
        let fun = self.l.select(qual, m, Span::NONE);
        let fun = self.l.positioned(fun);
        let info = self.l.u.info(m);
        let Type::Method { result, .. } = *self.l.u.types().get(info) else { unreachable!("a method") };
        let apply = self.l.apply(fun, args, result, Span::NONE);
        self.l.positioned(apply)
    }

    /// The definition of variable `sym` with its initializer, at the variable.
    fn val_def(&mut self, sym: SymbolId) -> TNodeId {
        let rhs = self.initializer[&sym];
        let name = self.l.u.symbol(sym).name.clone();
        let name = self.l.term_name(&name);
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
                self.l.block(&all, last, span)
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
