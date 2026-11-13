// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! The annotation tree an inferred type tree's annotated type came from: the one of the
//! definition, pattern variable or type ascription its right-hand side ends in.

use crate::tpd::{TNodeId, Tpd, Tree as T};
use rustc_hash::FxHashMap;
use tasty::{SymbolId, TypeId};

/// The most definitions with inferred types an annotated type is traced back through.
const MAX_HOPS: usize = 64;

/// The definitions of a tree pickled: the type tree of each, and the right-hand side of each
/// inferred type tree.
pub struct AnnotationOrigins {
    tpts: FxHashMap<SymbolId, TNodeId>,
    rhs_of: FxHashMap<TNodeId, TNodeId>,
}

impl AnnotationOrigins {
    /// Collects the definitions and pattern variables in `root`.
    pub fn new(tpd: &Tpd, root: TNodeId) -> Self {
        let mut origins = AnnotationOrigins { tpts: FxHashMap::default(), rhs_of: FxHashMap::default() };
        let mut stack = vec![root];
        while let Some(t) = stack.pop() {
            stack.extend(tpd.children(t));
            let (tpt, rhs) = match *tpd.tree(t) {
                T::ValDef { tpt, rhs, .. } | T::DefDef { tpt, rhs, .. } => (tpt, rhs),
                T::Bind { body, .. } => match *tpd.tree(body) {
                    T::Typed { tpt, .. } => (tpt, None),
                    _ => continue,
                },
                _ => continue,
            };
            if let Some(sym) = tpd.sym(t) {
                origins.tpts.insert(sym, tpt);
            }
            if let (T::TypeTree { inferred: true }, Some(rhs)) = (tpd.tree(tpt), rhs) {
                origins.rhs_of.insert(tpt, rhs);
            }
        }
        origins
    }

    /// The annotation tree of `annotated`, the type of inferred type tree `tpt`, where its
    /// right-hand side ends in a type ascription or a reference to a definition or pattern
    /// variable of this tree whose type tree gave it; a conditional or match only where all its
    /// branches but those of type `Nothing` give the same tree.
    pub fn of_inferred(&self, tpd: &Tpd, tpt: TNodeId, annotated: TypeId) -> Option<TNodeId> {
        self.of_tpt(tpd, tpt, annotated, MAX_HOPS)
    }

    fn of_tpt(&self, tpd: &Tpd, tpt: TNodeId, annotated: TypeId, hops: usize) -> Option<TNodeId> {
        if tpd.get(tpt).tpe != Some(annotated) {
            return None;
        }
        match *tpd.tree(tpt) {
            T::Annotated { annot, .. } => Some(annot),
            T::TypeTree { inferred: true } => {
                self.of_rhs(tpd, *self.rhs_of.get(&tpt)?, annotated, hops.checked_sub(1)?)
            }
            _ => tpd.tpt_annotation(tpt),
        }
    }

    fn of_rhs(&self, tpd: &Tpd, rhs: TNodeId, annotated: TypeId, hops: usize) -> Option<TNodeId> {
        match *tpd.tree(rhs) {
            T::Typed { tpt, .. } => self.of_tpt(tpd, tpt, annotated, hops),
            T::Block { expr, .. } => self.of_rhs(tpd, expr, annotated, hops),
            T::Apply { fun, .. } | T::TypeApply { fun, .. } => self.of_rhs(tpd, fun, annotated, hops),
            T::Ident { .. } | T::Select { .. } => {
                self.of_tpt(tpd, *self.tpts.get(&tpd.sym(rhs)?)?, annotated, hops)
            }
            T::If { thenp, elsep, .. } => self.of_branches(tpd, &[thenp, elsep], annotated, hops),
            T::Match { cases, .. } => {
                let bodies: Vec<TNodeId> = (tpd.items(cases).iter())
                    .filter_map(|&c| match *tpd.tree(c) {
                        T::CaseDef { body, .. } => Some(body),
                        _ => None,
                    })
                    .collect();
                self.of_branches(tpd, &bodies, annotated, hops)
            }
            _ => None,
        }
    }

    /// The one tree branches `branches` give, those of type `Nothing` left out.
    fn of_branches(
        &self,
        tpd: &Tpd,
        branches: &[TNodeId],
        annotated: TypeId,
        hops: usize,
    ) -> Option<TNodeId> {
        let mut trees = (branches.iter())
            .filter(|&&b| tpd.get(b).tpe != Some(tasty::NOTHING))
            .map(|&b| self.of_rhs(tpd, b, annotated, hops));
        let first = trees.next()??;
        trees.all(|t| t == Some(first)).then_some(first)
    }
}
