// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! The annotation tree an inferred type tree's annotated type came from: the one of the
//! definition or type ascription its right-hand side ends in.

use crate::tpd::{TNodeId, Tpd, Tree as T};
use rustc_hash::FxHashMap;
use tasty::{SymbolId, TypeId};

/// The definitions of a tree pickled: the type tree and right-hand side of each, and the
/// right-hand side of each inferred type tree.
pub struct AnnotationOrigins {
    defs: FxHashMap<SymbolId, (TNodeId, Option<TNodeId>)>,
    rhs_of: FxHashMap<TNodeId, TNodeId>,
}

impl AnnotationOrigins {
    /// Collects the definitions in `root`.
    pub fn new(tpd: &Tpd, root: TNodeId) -> Self {
        let mut origins = AnnotationOrigins { defs: FxHashMap::default(), rhs_of: FxHashMap::default() };
        let mut stack = vec![root];
        while let Some(t) = stack.pop() {
            if let T::ValDef { tpt, rhs, .. } | T::DefDef { tpt, rhs, .. } = *tpd.tree(t) {
                if let Some(sym) = tpd.sym(t) {
                    origins.defs.insert(sym, (tpt, rhs));
                }
                if let (T::TypeTree { inferred: true }, Some(rhs)) = (tpd.tree(tpt), rhs) {
                    origins.rhs_of.insert(tpt, rhs);
                }
            }
            stack.extend(tpd.children(t));
        }
        origins
    }

    /// The annotation tree of `annotated`, the type of inferred type tree `tpt`, where its
    /// right-hand side ends in a type ascription or a reference to a definition of this tree
    /// whose type tree gave it.
    pub fn of_inferred(&self, tpd: &Tpd, tpt: TNodeId, annotated: TypeId) -> Option<TNodeId> {
        let mut visited = Vec::new();
        self.of_tpt(tpd, tpt, annotated, &mut visited)
    }

    fn of_tpt(
        &self,
        tpd: &Tpd,
        tpt: TNodeId,
        annotated: TypeId,
        visited: &mut Vec<TNodeId>,
    ) -> Option<TNodeId> {
        if tpd.get(tpt).tpe != Some(annotated) || visited.contains(&tpt) {
            return None;
        }
        visited.push(tpt);
        match *tpd.tree(tpt) {
            T::Annotated { annot, .. } => Some(annot),
            T::TypeTree { inferred: true } => self.of_rhs(tpd, *self.rhs_of.get(&tpt)?, annotated, visited),
            _ => tpd.tpt_annotation(tpt),
        }
    }

    fn of_rhs(
        &self,
        tpd: &Tpd,
        rhs: TNodeId,
        annotated: TypeId,
        visited: &mut Vec<TNodeId>,
    ) -> Option<TNodeId> {
        match *tpd.tree(rhs) {
            T::Typed { tpt, .. } => self.of_tpt(tpd, tpt, annotated, visited),
            T::Block { expr, .. } => self.of_rhs(tpd, expr, annotated, visited),
            T::Apply { fun, .. } | T::TypeApply { fun, .. } => self.of_rhs(tpd, fun, annotated, visited),
            T::Ident { .. } | T::Select { .. } => {
                let (tpt, _) = *self.defs.get(&tpd.sym(rhs)?)?;
                self.of_tpt(tpd, tpt, annotated, visited)
            }
            _ => None,
        }
    }
}
