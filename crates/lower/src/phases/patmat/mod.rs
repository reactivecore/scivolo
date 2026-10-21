// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PatternMatcher.scala
//! Pattern matches compiled to tests, value definitions, labeled blocks and switches (dotty's
//! `PatternMatcher`): a match becomes a plan (`plan.rs`), optimized (`optimize.rs`), emitted as
//! trees (`emit.rs`).

mod emit;
mod extractors;
mod generic_tuples;
mod inline;
mod members;
mod merge;
mod plan;
mod seq_members;
mod sequences;
mod switch;
mod try_catch;
mod types;

use super::walk::children;
use crate::definitions::{Patmat, symbol};
use crate::{Lower, Result};
use plan::{Plan, PlanId};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::Span;
use tasty::tags::{INLINE, LAZY};
use tasty::{Flags, Kind, SymbolId, TypeId};
use typer::tpd::{MatchKind, TNodeId, Tree};

/// Compiles the matches in unit `root` in the order of its trees, inner matches first; those
/// outside methods with their variables owned by the constructor.
pub fn transform(l: &mut Lower, root: TNodeId, owner: SymbolId) -> Result<()> {
    let mut matches = Vec::new();
    let mut case_bodies = FxHashSet::default();
    let mut todo = vec![(root, owner, false)];
    while let Some((t, owner, visited)) = todo.pop() {
        if visited {
            match l.tpd.tree(t) {
                Tree::Match { kind: MatchKind::Sub, .. } if !case_bodies.contains(&t) => {
                    return crate::unsupported(l.tpd.span(t), "sub match");
                }
                Tree::Match { kind: MatchKind::Sub, .. } => {}
                Tree::Match { .. } => matches.push((t, owner)),
                _ => {}
            }
            matches.extend(try_catch::fallback(l, t, owner)?.map(|m| (m, owner)));
            continue;
        }
        let inner = match *l.tpd.tree(t) {
            Tree::CaseDef { body, .. } => {
                case_bodies.insert(body);
                owner
            }
            Tree::DefDef { .. } if is_inline_with_sub_match(l, t) => continue,
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
        let defs = l.patmat()?;
        Translator::new(l, m, owner, defs).translate(m)?;
    }
    Ok(())
}

/// Whether `t` defines an inline method, which erasure drops, with a sub match or a `summonFrom`
/// in its body.
fn is_inline_with_sub_match(l: &Lower, t: TNodeId) -> bool {
    fn has_sub_match(l: &Lower, t: TNodeId) -> bool {
        matches!(l.tpd.tree(t), Tree::Match { kind: MatchKind::Sub, .. } | Tree::Match { selector: None, .. })
            || children(l.tpd, t).into_iter().any(|c| has_sub_match(l, c))
    }
    l.tpd.sym(t).is_some_and(|s| crate::symbols::has(l.u, s, INLINE)) && has_sub_match(l, t)
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
    defs: Patmat,
}

impl<'t, 'a> Translator<'t, 'a> {
    fn new(l: &'t mut Lower<'a>, m: TNodeId, owner: SymbolId, defs: Patmat) -> Self {
        let mut t = Translator {
            l,
            defs,
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
        let rhs = self.drop_named_tuple(rhs);
        let tpe = tpe.unwrap_or_else(|| self.l.tpd.tpe(rhs));
        let name = self.l.fresh.next(self.l.names, "x");
        let name = self.l.names.text(name).to_owned();
        let var = self.l.u.define(symbol(&name, Kind::Val, self.owner, Flags::default()));
        self.l.u.set_info(var, tpe);
        self.generated.insert(var);
        self.initializer.insert(var, rhs);
        self.spans.insert(var, coord(super::drop_inlined::span_before_erasure(self.l, rhs)));
        let body = body(self, var)?;
        Ok(self.add(Plan::Let { sym: var, body }))
    }

    /// The plan `l: { expr(return l) }` for a fresh label `l`.
    fn alts_labeled(&mut self, expr: impl FnOnce(&mut Self, PlanId) -> Result<PlanId>) -> Result<PlanId> {
        let unit = crate::definitions::type_ref(self.l.u, self.l.defs.unit);
        let label = self.label("matchAlts", unit);
        let ret = self.add(Plan::Return { label });
        let expr = expr(self, ret)?;
        Ok(self.add(Plan::Labeled { sym: label, expr }))
    }

    fn is_generated(&self, s: SymbolId) -> bool {
        self.generated.contains(&s)
    }
}

/// The span of a symbol defined at `span`: its point (dotty's `Coord`).
pub fn coord(span: Span) -> Span {
    if span.exists() { Span::at(span.point()) } else { span }
}
