// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! The annotation tree an inferred type tree's annotated type came from: the one of the
//! definition, pattern variable or type ascription its right-hand side ends in.

use crate::tpd::{TNodeId, Tpd, Tree as T};
use rustc_hash::FxHashMap;
use syntax::source::Offset;
use tasty::{SymbolId, TypeId};

/// The most definitions with inferred types an annotated type is traced back through.
const MAX_HOPS: usize = 64;

/// The definitions of a tree pickled: the type tree of each, the right-hand side of each
/// inferred type tree, and the annotation of each annotated type tree by its span and type.
pub struct AnnotationOrigins {
    tpts: FxHashMap<SymbolId, TNodeId>,
    rhs_of: FxHashMap<TNodeId, TNodeId>,
    annotated_at: Vec<(Offset, Offset, TypeId, TNodeId)>,
}

impl AnnotationOrigins {
    /// Collects the definitions, pattern variables and annotated type trees in `root`.
    pub fn new(tpd: &Tpd, root: TNodeId) -> Self {
        let mut origins = AnnotationOrigins {
            tpts: FxHashMap::default(),
            rhs_of: FxHashMap::default(),
            annotated_at: Vec::new(),
        };
        let mut stack = vec![root];
        while let Some(t) = stack.pop() {
            stack.extend(tpd.children(t));
            if let (T::Annotated { annot, .. }, Some(tpe)) = (tpd.tree(t), tpd.get(t).tpe) {
                let span = tpd.span(t);
                origins.annotated_at.push((span.start(), span.end(), tpe, *annot));
            }
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

    /// The annotation tree of `annotated`, of annotation class `annot` and a part of the type of
    /// type tree `tpt` pickled, `Some(None)` if several may be: of a type tree derived from a
    /// written one (a parameter accessor's from its parameter's), the annotated type trees of the
    /// written one's span with that type, else with that annotation class (a class parameter's
    /// type of the class's type parameters); of an inferred type tree of type `annotated`, the
    /// tree its right-hand side gives where it ends in a type ascription or a reference to a
    /// definition or pattern variable of this tree, a conditional or match only where all its
    /// branches but those of type `Nothing` give the same tree.
    pub fn of_type_tree(
        &self,
        tpd: &Tpd,
        tpt: TNodeId,
        annotated: TypeId,
        annot: TypeId,
    ) -> Option<Option<TNodeId>> {
        match *tpd.tree(tpt) {
            T::TypeTree { inferred: false } => (self.derived(tpd, tpt, |t, _| t == annotated))
                .or_else(|| self.derived(tpd, tpt, |_, a| tpd.get(a).tpe == Some(annot))),
            _ => self.of_tpt(tpd, tpt, annotated, MAX_HOPS).map(Some),
        }
    }

    /// The annotation tree within the span of type tree `tpt` whose annotated type and tree
    /// `matching` holds of, `Some(None)` if several, `None` if none.
    fn derived(
        &self,
        tpd: &Tpd,
        tpt: TNodeId,
        matching: impl Fn(TypeId, TNodeId) -> bool,
    ) -> Option<Option<TNodeId>> {
        let span = tpd.span(tpt);
        if !span.exists() || span.start() == span.end() {
            return None;
        }
        let mut annots = (self.annotated_at.iter())
            .filter(|&&(start, end, t, annot)| {
                start >= span.start() && end <= span.end() && matching(t, annot)
            })
            .map(|&(.., annot)| annot);
        let first = annots.next()?;
        Some(annots.all(|a| a == first).then_some(first))
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
            T::TypeTree { inferred: false } => {
                (self.derived(tpd, tpt, |t, _| t == annotated).flatten()).or_else(|| tpd.tpt_annotation(tpt))
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
