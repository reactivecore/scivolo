// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! The annotation tree an inferred type tree's annotated type, or a part of it, came from: the one
//! of the definition, pattern variable or type ascription its right-hand side ends in.

mod nested;
mod type_args;

use crate::tpd::{TNodeId, Tpd, Tree as T};
use nested::ParamOf;
use rustc_hash::FxHashMap;
use syntax::source::Offset;
use tasty::{SymbolId, TypeId, Universe};

/// The most definitions with inferred types an annotated type is traced back through.
const MAX_HOPS: usize = 64;

/// The annotation tree found: `Some(None)` if several, `None` if none.
type Found = Option<Option<TNodeId>>;

/// The definitions of a tree pickled: the type tree of each, the right-hand side of each
/// inferred or cast type tree, the parameter type trees of each method and the eta-expanded
/// parameters, the arguments and expected type of each inferred type argument, and the annotation
/// of each annotated type tree by its span and type.
pub struct AnnotationOrigins {
    tpts: FxHashMap<SymbolId, TNodeId>,
    rhs_of: FxHashMap<TNodeId, TNodeId>,
    params: FxHashMap<SymbolId, Vec<Vec<TNodeId>>>,
    eta: FxHashMap<TNodeId, ParamOf>,
    call_args: FxHashMap<TNodeId, Vec<TNodeId>>,
    pts: FxHashMap<TNodeId, TNodeId>,
    annotated_at: Vec<(Offset, Offset, TypeId, TNodeId)>,
}

impl AnnotationOrigins {
    /// Collects the definitions, pattern variables, ascriptions, casts and annotated type trees
    /// in `root`.
    pub fn new(u: &Universe, tpd: &Tpd, root: TNodeId) -> Self {
        let mut origins = AnnotationOrigins {
            tpts: FxHashMap::default(),
            rhs_of: FxHashMap::default(),
            params: FxHashMap::default(),
            eta: FxHashMap::default(),
            call_args: FxHashMap::default(),
            pts: FxHashMap::default(),
            annotated_at: Vec::new(),
        };
        let mut stack = vec![root];
        while let Some(t) = stack.pop() {
            stack.extend(tpd.children(t));
            if let (T::Annotated { annot, .. }, Some(tpe)) = (tpd.tree(t), tpd.get(t).tpe) {
                let span = tpd.span(t);
                origins.annotated_at.push((span.start(), span.end(), tpe, *annot));
            }
            origins.add_ascription_or_cast(u, tpd, t);
            origins.add_params(tpd, t);
            origins.add_type_args(u, tpd, t);
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
    /// type of the class's type parameters); of an inferred type tree, a flow-typing cast's or an
    /// eta-expanded parameter's, the tree its right-hand side, qualifier or method parameter gives
    /// (`of_tpt`).
    pub fn of_type_tree(
        &self,
        u: &Universe,
        tpd: &Tpd,
        tpt: TNodeId,
        annotated: TypeId,
        annot: TypeId,
    ) -> Found {
        match *tpd.tree(tpt) {
            T::TypeTree { inferred: false }
                if !self.rhs_of.contains_key(&tpt) && !self.eta.contains_key(&tpt) =>
            {
                (self.derived(tpd, tpt, |t, _| t == annotated))
                    .or_else(|| self.derived(tpd, tpt, |_, a| tpd.get(a).tpe == Some(annot)))
            }
            _ => self.of_tpt(&Query { u, tpd, annotated }, tpt, MAX_HOPS),
        }
    }

    /// The annotation tree of `annotated` that expression `expr` gives, `Some(None)` if several
    /// may (a match selector's, for the types of its patterns).
    pub fn of_expr(&self, u: &Universe, tpd: &Tpd, expr: TNodeId, annotated: TypeId) -> Found {
        self.of_rhs(&Query { u, tpd, annotated }, expr, MAX_HOPS)
    }

    /// The tree type tree `tpt` gives `q.annotated`, the whole of its type or a part: an
    /// annotated type tree of that type, else the one annotated type tree of that type in its
    /// span; of an inferred type tree, a flow-typing cast's, an eta-expanded parameter's or an
    /// inferred type argument's, the tree its right-hand side, qualifier, method parameter or
    /// arguments give.
    fn of_tpt(&self, q: &Query, tpt: TNodeId, hops: usize) -> Found {
        let (tpd, annotated) = (q.tpd, q.annotated);
        if let Some(&rhs) = self.rhs_of.get(&tpt) {
            return self.of_rhs(q, rhs, hops.checked_sub(1)?);
        }
        if let Some(param) = self.eta_param(tpt) {
            return self.of_tpt(q, param, hops.checked_sub(1)?);
        }
        if let Some(found) = self.of_type_arg(q, tpt, hops) {
            return found;
        }
        let within = || self.derived(tpd, tpt, |t, _| t == annotated);
        if tpd.get(tpt).tpe != Some(annotated) {
            return within();
        }
        match *tpd.tree(tpt) {
            T::Annotated { annot, .. } => Some(Some(annot)),
            T::TypeTree { inferred: false } => {
                within().flatten().or_else(|| tpd.tpt_annotation(tpt)).map(Some)
            }
            _ => tpd.tpt_annotation(tpt).map(Some).or_else(within),
        }
    }

    /// The tree right-hand side `rhs` gives `q.annotated`: of a type ascription its type tree's,
    /// of a reference to a definition or pattern variable of this tree its type tree's, else its
    /// qualifier's, of a call its method's, of a conditional or match the one all its branches
    /// give whose types have that part.
    fn of_rhs(&self, q: &Query, rhs: TNodeId, hops: usize) -> Found {
        let tpd = q.tpd;
        match *tpd.tree(rhs) {
            T::Typed { tpt, .. } => self.of_tpt(q, tpt, hops),
            T::Block { expr, .. } => self.of_rhs(q, expr, hops),
            T::Apply { fun, .. } => self.of_rhs(q, fun, hops),
            T::TypeApply { fun, .. } => (self.of_rhs(q, fun, hops))
                .or_else(|| self.of_inferred_type_args(q, rhs, hops.checked_sub(1)?)),
            T::Ident { .. } => self.of_tpt(q, *self.tpts.get(&tpd.sym(rhs)?)?, hops),
            T::Select { qualifier, .. } => {
                let member = tpd.sym(rhs).and_then(|s| self.tpts.get(&s));
                (member.and_then(|&tpt| self.of_tpt(q, tpt, hops)))
                    .or_else(|| self.of_rhs(q, qualifier, hops.checked_sub(1)?))
            }
            T::If { thenp, elsep, .. } => self.of_branches(q, &[thenp, elsep], hops),
            T::Match { cases, .. } => {
                let bodies: Vec<TNodeId> = (tpd.items(cases).iter())
                    .filter_map(|&c| match *tpd.tree(c) {
                        T::CaseDef { body, .. } => Some(body),
                        _ => None,
                    })
                    .collect();
                self.of_branches(q, &bodies, hops)
            }
            _ => None,
        }
    }

    /// The one tree branches `branches` give, those whose types have no part `q.annotated` left
    /// out.
    fn of_branches(&self, q: &Query, branches: &[TNodeId], hops: usize) -> Found {
        let mut trees = (branches.iter())
            .filter(|&&b| q.tpd.get(b).tpe.is_none_or(|t| nested::may_have_part(q.u, t, q.annotated)))
            .map(|&b| self.of_rhs(q, b, hops));
        let first = trees.next()??;
        trees.all(|t| t == Some(first)).then_some(first)
    }
}

/// The annotated type an annotation tree is looked for, and the trees and types it is in.
pub struct Query<'a> {
    u: &'a Universe,
    tpd: &'a Tpd,
    annotated: TypeId,
}
