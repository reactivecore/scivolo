// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/InlineReducer.scala
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! Inline matches reduced to the case their scrutinee selects (dotty's
//! `InlineTyper.typedMatchFinish` and `InlineReducer`).

mod bindings;
mod cases;
mod patterns;
mod projection;

use super::{Inlining, Unsupported};
use crate::tpd::{List, TNodeId, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use tasty::tags::GIVEN;
use tasty::{SymbolId, TypeId};

/// The reduction of one inline match in an expansion.
pub struct Reducer<'r, 'a> {
    pub i: &'r mut Inlining<'a>,
    /// The inline arguments by proxy, as the retyper replaces them.
    args: &'r FxHashMap<SymbolId, TNodeId>,
    owner: SymbolId,
    /// The right-hand side of each binding the reduction made (dotty's `defTree`).
    rhs_of: FxHashMap<SymbolId, TNodeId>,
    /// The bindings of erased impure values, dropped from the result.
    unusable: FxHashSet<SymbolId>,
    /// Whether the match is a `summonFrom`, its patterns matching implicit arguments.
    implicit: bool,
    /// The types the type variables of the patterns matched so far stand for.
    type_bindings: Vec<(SymbolId, TypeId)>,
}

impl Inlining<'_> {
    /// Inline match `m` of retyped selector `sel`, none for a `summonFrom`, and `cases`, owned by
    /// `owner`, as the retyped body of the case it selects with the bindings of its pattern.
    pub fn reduce_inline_match(
        &mut self,
        m: TNodeId,
        sel: Option<TNodeId>,
        cases: List,
        args: &FxHashMap<SymbolId, TNodeId>,
        owner: SymbolId,
    ) -> Result<TNodeId, Unsupported> {
        let sel = sel.map(|s| self.sel_typed(s));
        let cases = self.tpd.items(cases).to_vec();
        let (rhs_of, unusable, implicit) = (FxHashMap::default(), FxHashSet::default(), sel.is_none());
        let type_bindings = Vec::new();
        let mut r = Reducer { i: self, args, owner, rhs_of, unusable, implicit, type_bindings };
        match sel {
            Some(sel) => {
                let sel_type = r.i.tpd.tpe(sel);
                r.reduce_expr(m, sel, sel_type, &cases)
            }
            None => {
                let reduced = r.reduce_cases(None, tasty::NOTHING, &cases)?;
                r.selected(m, reduced)
            }
        }
    }

    /// Selector `sel` without its type ascriptions.
    fn sel_typed(&mut self, sel: TNodeId) -> TNodeId {
        let span = self.tpd.span(sel);
        let tree = match self.tpd.tree(sel).clone() {
            T::Typed { expr, .. } => return self.sel_typed(expr),
            T::Block { stats, expr } if stats.is_empty() => T::Block { stats, expr: self.sel_typed(expr) },
            T::Inlined { call, bindings, expansion } if bindings.is_empty() => {
                T::Inlined { call, bindings, expansion: self.sel_typed(expansion) }
            }
            _ => return sel,
        };
        let (T::Block { expr: inner, .. } | T::Inlined { expansion: inner, .. }) = tree else { return sel };
        let tpe = self.tpd.get(inner).tpe;
        self.tpd.node(tree, span, tpe, None)
    }
}

impl Reducer<'_, '_> {
    /// Match `m` on `sel` of type `sel_type` reduced, inside the blocks `sel` flattens to.
    fn reduce_expr(
        &mut self,
        m: TNodeId,
        sel: TNodeId,
        sel_type: TypeId,
        cases: &[TNodeId],
    ) -> Result<TNodeId, Unsupported> {
        let flat = self.flatten(sel)?;
        if let T::Block { stats, expr } = *self.i.tpd.tree(flat) {
            let stats = self.i.tpd.items(stats).to_vec();
            let reduced = self.reduce_expr(m, expr, sel_type, cases)?;
            let span = self.i.tpd.span(sel);
            return self.i.block(&stats, reduced, span);
        }
        let reduced = self.reduce(sel, sel_type, cases)?;
        self.selected(m, reduced)
    }

    /// The retyped body of the case of match `m` that `reduced` holds with its bindings.
    fn selected(
        &mut self,
        m: TNodeId,
        reduced: Option<(Vec<TNodeId>, TNodeId)>,
    ) -> Result<TNodeId, Unsupported> {
        let Some((bindings, rhs)) = reduced else {
            return Err((self.i.tpd.span(m), "inline match that does not reduce"));
        };
        let rhs = self.drop_synthetic_ascription(rhs)?;
        let rhs = match *self.i.tpd.tree(rhs) {
            T::Typed { expr, .. } if self.i.tpd.span(rhs).is_synthetic() => expr,
            _ => rhs,
        };
        let rhs = self.i.const_to_literal(rhs);
        let (used, rhs) = self.i.drop_unused(bindings.clone(), rhs, self.owner)?;
        let rhs = match *self.i.tpd.tree(rhs) {
            T::Block { stats, expr } if self.implicit && stats.is_empty() && !used.is_empty() => expr,
            _ => rhs,
        };
        let rhs = self.i.seq(&used, rhs)?;
        let mark = self.with_givens(&used);
        let retyped = self.i.retype(rhs, self.args, self.owner);
        self.i.case_givens.truncate(mark);
        retyped
    }

    /// The givens among `bindings` of a `summonFrom` case added to the case givens in scope; the
    /// number of those before.
    fn with_givens(&mut self, bindings: &[TNodeId]) -> usize {
        let mark = self.i.case_givens.len();
        if self.implicit {
            let syms = bindings.iter().filter_map(|&b| self.i.tpd.sym(b));
            let givens: Vec<SymbolId> = syms.filter(|&s| self.i.u.symbol(s).flags.has(GIVEN)).collect();
            self.i.case_givens.extend(givens);
        }
        mark
    }

    /// Body `rhs` of a selected case without a synthetic ascription or cast of its expression.
    fn drop_synthetic_ascription(&mut self, rhs: TNodeId) -> Result<TNodeId, Unsupported> {
        let T::Block { stats, expr } = *self.i.tpd.tree(rhs) else { return Ok(rhs) };
        if !self.i.tpd.span(expr).is_synthetic() {
            return Ok(rhs);
        }
        let inner = match *self.i.tpd.tree(expr) {
            T::Typed { expr, .. } => expr,
            T::TypeApply { fun, .. } => match (self.i.tpd.tree(fun).clone(), self.i.tpd.sym(fun)) {
                (T::Select { qualifier, .. }, Some(s)) if self.i.u.symbol(s).name == "asInstanceOf" => {
                    qualifier
                }
                _ => return Ok(rhs),
            },
            _ => return Ok(rhs),
        };
        let stats = self.i.tpd.items(stats).to_vec();
        let span = self.i.envelope(&stats, inner);
        self.i.block(&stats, inner, span)
    }

    /// Inlined tree `t` with bindings or a block as a block of inlined trees without bindings
    /// (dotty's `flattenInlineBlock`).
    fn flatten(&mut self, t: TNodeId) -> Result<TNodeId, Unsupported> {
        let T::Inlined { call, bindings, expansion } = *self.i.tpd.tree(t) else { return Ok(t) };
        let (stats, expr) = match *self.i.tpd.tree(expansion) {
            _ if !bindings.is_empty() => (self.i.tpd.items(bindings).to_vec(), expansion),
            T::Block { stats, expr } => (self.i.tpd.items(stats).to_vec(), expr),
            _ => return Ok(t),
        };
        let mut flat = Vec::new();
        for s in stats {
            let (span, tpe, sym) = (self.i.tpd.span(s), self.i.tpd.get(s).tpe, self.i.tpd.sym(s));
            flat.push(match self.i.tpd.tree(s).clone() {
                T::ValDef { name, tpt, rhs: Some(r) } => {
                    let rhs = Some(self.inlined(call, r));
                    self.i.tpd.node(T::ValDef { name, tpt, rhs }, span, tpe, sym)
                }
                T::DefDef { name, paramss, tpt, rhs: Some(r) } => {
                    let rhs = Some(self.inlined(call, r));
                    self.i.tpd.node(T::DefDef { name, paramss, tpt, rhs }, span, tpe, sym)
                }
                _ => self.inlined(call, s),
            });
        }
        let expr = self.inlined(call, expr);
        let expr = self.flatten(expr)?;
        let span = self.i.tpd.span(t);
        self.i.block(&flat, expr, span)
    }

    /// `t` inlined from `call` without bindings, at its own span.
    fn inlined(&mut self, call: Option<TNodeId>, t: TNodeId) -> TNodeId {
        let (span, tpe) = (self.i.tpd.span(t), self.i.tpd.get(t).tpe);
        let bindings = self.i.tpd.list(&[]);
        self.i.tpd.node(T::Inlined { call, bindings, expansion: t }, span, tpe, None)
    }
}
