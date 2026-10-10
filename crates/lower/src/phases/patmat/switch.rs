//! Sequences of equality tests against distinct `Int`-range or `String` literals emitted as a
//! switch: a match on an `Int` or `String` whose patterns are literals (dotty's
//! `collectSwitchCases`, `emitSwitchMatch`).

use super::Translator;
use super::plan::{Plan, PlanId, Test};
use crate::phases::walk::same;
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Erased, Kind};
use typer::tpd::{TNodeId, Tree};

/// The fewest alternatives, with the default, a switch is emitted for.
const MIN_SWITCH_CASES: usize = 4;

/// A case of a switch: its literals, none for the default, and its plan.
type Case = (Vec<TNodeId>, PlanId);

impl Translator<'_, '_> {
    /// Sequence `p` as a switch on `scrutinee`, if it has enough cases.
    pub fn switch(&mut self, scrutinee: TNodeId, p: PlanId) -> Option<TNodeId> {
        let erased = self.l.u.erase(self.l.tpd.tpe(scrutinee));
        let string = Erased::Class("java.lang.String".into());
        if !matches!(erased, Erased::Prim('i' | 'b' | 's' | 'c')) && erased != string {
            return None;
        }
        let mut seen = Vec::new();
        let cases = self.switch_cases(scrutinee, p, &mut seen);
        let alternatives: usize = cases.iter().map(|(alts, _)| alts.len()).sum();
        (alternatives + 1 >= MIN_SWITCH_CASES).then(|| self.emit_switch(scrutinee, &erased, cases))
    }

    fn switch_cases(&self, scrutinee: TNodeId, p: PlanId, seen: &mut Vec<Constant>) -> Vec<Case> {
        if let Plan::Seq { head, tail } = *self.plan(p) {
            if let Plan::Test { test: Test::Equal(tree), scrutinee: s, on_success, .. } = *self.plan(head)
                && same(self.l.tpd, s, scrutinee)
                && !self.can_fall_through(on_success)
                && self.new_constant(tree, seen)
            {
                let mut out = vec![(vec![tree], on_success)];
                out.extend(self.switch_cases(scrutinee, tail, seen));
                return out;
            }
            if let Some(case) = self.alternatives(scrutinee, head, seen) {
                let mut out = vec![case];
                out.extend(self.switch_cases(scrutinee, tail, seen));
                return out;
            }
        }
        vec![(Vec::new(), p)]
    }

    /// The literals and plan of `l: { l2: { tests returning l2 ... return l }; on_success }`.
    fn alternatives(&self, scrutinee: TNodeId, p: PlanId, seen: &mut Vec<Constant>) -> Option<Case> {
        let Plan::Labeled { sym: outer, expr } = *self.plan(p) else { return None };
        let Plan::Seq { head, tail: on_success } = *self.plan(expr) else { return None };
        let Plan::Labeled { sym: inner, expr: mut rest } = *self.plan(head) else { return None };
        if self.can_fall_through(on_success) {
            return None;
        }
        let mut alts = Vec::new();
        loop {
            match *self.plan(rest) {
                Plan::Seq { head, tail } => {
                    let Plan::Test { test: Test::Equal(tree), scrutinee: s, on_success, .. } =
                        *self.plan(head)
                    else {
                        return None;
                    };
                    let returns = matches!(*self.plan(on_success), Plan::Return { label } if label == inner);
                    if !(returns && same(self.l.tpd, s, scrutinee) && self.new_constant(tree, seen)) {
                        return None;
                    }
                    alts.push(tree);
                    rest = tail;
                }
                Plan::Return { label } if label == outer => return Some((alts, on_success)),
                _ => return None,
            }
        }
    }

    /// Whether `t` is an `Int`-range or `String` literal not in `seen`, then added to it.
    fn new_constant(&self, t: TNodeId, seen: &mut Vec<Constant>) -> bool {
        let Tree::Literal { ref value } = *self.l.tpd.tree(t) else { return false };
        let switchable = matches!(value, Constant::Char(_) | Constant::Int(_) | Constant::String(_));
        if !switchable || seen.contains(value) {
            return false;
        }
        seen.push(value.clone());
        true
    }

    /// `scrutinee match { case lit | ... => ...; case _ => ... }` on an `Int` or `String`.
    fn emit_switch(&mut self, scrutinee: TNodeId, erased: &Erased, cases: Vec<Case>) -> TNodeId {
        let selector = match erased {
            Erased::Prim(c) if *c != 'i' => self.int_of(scrutinee),
            _ => scrutinee,
        };
        let scrutinee_tpe = self.l.tpd.tpe(selector);
        let mut defs = Vec::new();
        for (alts, plan) in cases {
            let pat = match alts.as_slice() {
                [] => self.l.wildcard(scrutinee_tpe),
                &[alt] => self.int_literal(alt),
                alts => {
                    let trees: Vec<TNodeId> = alts.iter().map(|&a| self.int_literal(a)).collect();
                    let trees = self.l.tpd.list(&trees);
                    self.node(Tree::Alternative { trees }, scrutinee_tpe, None)
                }
            };
            let body = self.emit(plan);
            let tpe = self.l.tpd.tpe(body);
            defs.push(self.node(Tree::CaseDef { pat, guard: None, body }, tpe, None));
        }
        let cases = self.l.tpd.list(&defs);
        let nothing = self.nothing_type();
        self.node(Tree::Match { selector, cases }, nothing, None)
    }

    /// `t.toInt`, of a `Char`, `Byte` or `Short`.
    fn int_of(&mut self, t: TNodeId) -> TNodeId {
        let tpe = self.l.u.widen(self.l.tpd.tpe(t));
        let class = self.l.u.class_symbol(tpe).expect("a primitive class");
        let decls = self.l.u.decl(class, "toInt").unwrap_or_default();
        let to_int = decls.into_iter().find(|&d| self.l.u.symbol(d).kind == Kind::Def).expect("`toInt`");
        let select = self.l.select(t, to_int, Span::NONE);
        let int = crate::definitions::type_ref(self.l.u, self.l.defs.patmat.int);
        self.l.tpd.get_mut(select).tpe = Some(int);
        self.l.positioned(select)
    }

    /// Literal `t` as an `Int` literal unless `Int` or `String`.
    fn int_literal(&mut self, t: TNodeId) -> TNodeId {
        let Tree::Literal { value: Constant::Char(c) } = *self.l.tpd.tree(t) else { return t };
        let span = self.l.tpd.span(t);
        self.l.build().literal(Constant::Int(i32::from(c)), span)
    }
}
