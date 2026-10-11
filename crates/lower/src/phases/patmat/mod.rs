//! Pattern matches compiled to tests, value definitions, labeled blocks and switches (dotty's
//! `PatternMatcher`): a match becomes a plan (`plan.rs`), optimized (`optimize.rs`), emitted as
//! trees (`emit.rs`).

mod emit;
mod extractors;
mod inline;
mod members;
mod merge;
mod plan;
mod sequences;
mod switch;
mod try_catch;
mod types;

use super::walk::children;
use crate::definitions::symbol;
use crate::{Lower, Result};
use plan::{Plan, PlanId};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::Span;
use tasty::tags::LAZY;
use tasty::{Flags, Kind, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Compiles the matches in unit `root` in the order of its trees, inner matches first; those
/// outside methods with their variables owned by the constructor.
pub fn transform(l: &mut Lower, root: TNodeId, owner: SymbolId) -> Result<()> {
    let mut matches = Vec::new();
    let mut todo = vec![(root, owner, false)];
    while let Some((t, owner, visited)) = todo.pop() {
        if visited {
            if matches!(l.tpd.tree(t), Tree::Match { .. }) {
                matches.push((t, owner));
            }
            matches.extend(try_catch::fallback(l, t, owner)?.map(|m| (m, owner)));
            continue;
        }
        let inner = match *l.tpd.tree(t) {
            Tree::DefDef { .. } | Tree::TypeDef { .. } => l.tpd.sym(t).unwrap_or(owner),
            Tree::ValDef { .. } => {
                l.tpd.sym(t).filter(|&s| crate::symbols::has(l.u, s, LAZY)).unwrap_or(owner)
            }
            Tree::Template { constr, .. } => l.tpd.sym(constr).unwrap_or(owner),
            _ => owner,
        };
        todo.push((t, owner, true));
        todo.extend(children(l.tpd, t).into_iter().rev().map(|c| (c, inner, false)));
    }
    for (m, owner) in matches {
        Translator::new(l, m, owner).translate(m)?;
    }
    Ok(())
}

/// The translation of one match: its plans, the variables it defines with their initial values
/// and spans, and the label its result exits.
pub struct Translator<'t, 'a> {
    pub l: &'t mut Lower<'a>,
    plans: Vec<Plan>,
    initializer: FxHashMap<SymbolId, TNodeId>,
    spans: FxHashMap<SymbolId, Span>,
    generated: FxHashSet<SymbolId>,
    /// The variables known not to be null: those of type patterns.
    non_null: FxHashSet<SymbolId>,
    owner: SymbolId,
    result_label: SymbolId,
}

impl<'t, 'a> Translator<'t, 'a> {
    fn new(l: &'t mut Lower<'a>, m: TNodeId, owner: SymbolId) -> Self {
        let mut t = Translator {
            l,
            plans: Vec::new(),
            initializer: FxHashMap::default(),
            spans: FxHashMap::default(),
            generated: FxHashSet::default(),
            non_null: FxHashSet::default(),
            owner,
            result_label: owner,
        };
        let tpe = t.l.tpd.tpe(m);
        t.result_label = t.label("matchResult", tpe);
        t
    }

    /// Replaces match `m` by its translation.
    fn translate(mut self, m: TNodeId) -> Result<()> {
        let plan = self.match_plan(m)?;
        let plan = self.merge_tests(plan);
        let plan = self.inline_vars(plan);
        let expr = self.emit(plan);
        let node = self.l.tpd.get_mut(m);
        (node.tree, node.sym, node.span) = (Tree::Labeled { expr }, Some(self.result_label), Span::NONE);
        self.l.positioned(m);
        Ok(())
    }

    fn add(&mut self, plan: Plan) -> PlanId {
        self.plans.push(plan);
        PlanId(self.plans.len() - 1)
    }

    /// A fresh label `<prefix>N` of type `tpe`.
    fn label(&mut self, prefix: &'static str, tpe: TypeId) -> SymbolId {
        let name = self.l.fresh.next(self.l.names, prefix);
        let name = self.l.names.text(name).to_owned();
        let label = self.l.u.define(symbol(&name, Kind::Val, self.owner, Flags::default()));
        self.l.u.set_info(label, tpe);
        label
    }

    /// The plan `let x = rhs in body(x)` for a fresh variable `x` of type `tpe`, else of `rhs`'s.
    fn let_abstract(
        &mut self,
        rhs: TNodeId,
        tpe: Option<TypeId>,
        body: impl FnOnce(&mut Self, SymbolId) -> Result<PlanId>,
    ) -> Result<PlanId> {
        let tpe = tpe.unwrap_or_else(|| self.l.tpd.tpe(rhs));
        let name = self.l.fresh.next(self.l.names, "x");
        let name = self.l.names.text(name).to_owned();
        let var = self.l.u.define(symbol(&name, Kind::Val, self.owner, Flags::default()));
        self.l.u.set_info(var, tpe);
        self.generated.insert(var);
        self.initializer.insert(var, rhs);
        self.spans.insert(var, self.l.tpd.span(rhs));
        let body = body(self, var)?;
        Ok(self.add(Plan::Let { sym: var, body }))
    }

    /// The plan `l: { expr(return l) }` for a fresh label `l`.
    fn alts_labeled(&mut self, expr: impl FnOnce(&mut Self, PlanId) -> Result<PlanId>) -> Result<PlanId> {
        let unit = crate::definitions::type_ref(self.l.u, self.l.defs.patmat.unit);
        let label = self.label("matchAlts", unit);
        let ret = self.add(Plan::Return { label });
        let expr = expr(self, ret)?;
        Ok(self.add(Plan::Labeled { sym: label, expr }))
    }

    fn is_generated(&self, s: SymbolId) -> bool {
        self.generated.contains(&s)
    }
}
