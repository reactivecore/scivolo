//! The type trees of a super accessor's member, whose annotation trees the accessor's parameter
//! and result types take.

use super::{Refusal, TreePickler};
use crate::tpd::{TNodeId, Tpd, Tree as T};
use tasty::{Kind, Type, TypeId};

/// Collects the typed trees of each term parameter's definition of method definition `def`, in
/// order, and of its result type tree; `None` if `def` is no method definition.
pub fn member_tpts(tpd: &Tpd, def: TNodeId) -> Option<Vec<Vec<TNodeId>>> {
    let T::DefDef { tpt, .. } = *tpd.tree(def) else { return None };
    let params = tpd.children(def).into_iter().filter(|&c| matches!(tpd.tree(c), T::ValDef { .. }));
    Some(params.chain([tpt]).map(|t| subtree(tpd, t)).collect())
}

/// The typed trees of `t` and its descendants.
fn subtree(tpd: &Tpd, t: TNodeId) -> Vec<TNodeId> {
    let (mut todo, mut trees) = (vec![t], Vec::new());
    while let Some(t) = todo.pop() {
        if tpd.get(t).tpe.is_some() {
            trees.push(t);
        }
        todo.extend(tpd.children(t));
    }
    trees
}

impl TreePickler<'_> {
    /// Pickles `pickle` inside a super accessor's parameter or result of member type trees `tpts`.
    pub fn with_member_tpts(
        &mut self,
        tpts: Vec<TNodeId>,
        pickle: &mut dyn FnMut(&mut Self) -> Result<(), Refusal>,
    ) -> Result<(), Refusal> {
        let outer = std::mem::replace(&mut self.member_tpts, tpts.clone());
        let pickled = self.with_typed_tpts(tpts, pickle);
        self.member_tpts = outer;
        pickled
    }

    /// The annotation tree of class type `annot` on `parent` of the member type trees of the super
    /// accessor pickled now, whose types are seen from the accessor's class (dotty's `memberInfo`
    /// keeps the member's annotation trees), of those on a type of `parent`'s class if any;
    /// `Some(None)` if they give several trees, one with arguments. Refuses a tree that refers to
    /// type parameters, which seeing from the class would map.
    pub fn member_annotation(
        &mut self,
        parent: TypeId,
        annot: TypeId,
    ) -> Option<Result<Option<TNodeId>, Refusal>> {
        let class = self.u.class_symbol(annot)?;
        let parent_class = self.u.class_symbol(parent);
        let (mut trees, mut on_parent) = (Vec::new(), Vec::new());
        for t in self.member_tpts.clone() {
            let &T::Annotated { annot: tree, .. } = self.tpd.tree(t) else { continue };
            let tpe = self.tpd.tpe(tree);
            if self.u.class_symbol(tpe) != Some(class) || trees.contains(&tree) {
                continue;
            }
            trees.push(tree);
            let Type::Annotated(p, _) = *self.u.ty(self.tpd.tpe(t)) else { continue };
            if self.u.class_symbol(p) == parent_class {
                on_parent.push(tree);
            }
        }
        if !on_parent.is_empty() {
            trees = on_parent;
        }
        let first = *trees.first()?;
        if trees.len() > 1 && trees.iter().any(|&t| self.has_arguments(t, annot)) {
            return Some(Ok(None));
        }
        if self.mentions_type_params(first) {
            return Some(Err("a super accessor's annotation tree that refers to type parameters"));
        }
        Some(Ok(Some(first)))
    }

    /// Whether a type of tree `tree` or its descendants refers to a type parameter.
    fn mentions_type_params(&mut self, tree: TNodeId) -> bool {
        let mut found = false;
        for t in subtree(self.tpd, tree) {
            let tpe = self.tpd.tpe(t);
            self.u.map_type(tpe, &mut |u, x| {
                let is_param = matches!(u.sym_ref(x), Some((_, s)) if u.symbol(s).kind == Kind::TypeParam);
                found |= is_param || matches!(u.ty(x), Type::Param(..));
                None
            });
        }
        found
    }
}
