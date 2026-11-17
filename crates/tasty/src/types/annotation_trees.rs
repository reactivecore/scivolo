//! The annotation trees of annotated types read from TASTy, which dotty's `AnnotatedType` keeps:
//! an annotated type with a tree is told from the same type with another tree or none.

use super::{Type, TypeId, Types};
use crate::{FileId, NodeId};
use rustc_hash::FxHashMap;

/// The annotation term of an annotated type in a TASTy file.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AnnotationTree {
    pub file: FileId,
    pub node: NodeId,
}

/// The annotated types that carry an annotation tree, by type and tree.
#[derive(Default)]
pub struct AnnotationTrees {
    trees: FxHashMap<TypeId, AnnotationTree>,
    ids: FxHashMap<(Type, AnnotationTree), TypeId>,
}

impl Types {
    /// Returns the id of annotated type `t` with annotation tree `tree`: one id per type and tree,
    /// other than `t`'s without a tree, as dotty's `AnnotatedType.eql` compares annotations by
    /// identity.
    pub fn annotated_with_tree(&mut self, t: Type, tree: AnnotationTree) -> TypeId {
        if let Some(&id) = self.annotation_trees.ids.get(&(t, tree)) {
            return id;
        }
        let id = TypeId(self.all.len() as u32);
        self.all.push(t);
        self.annotation_trees.ids.insert((t, tree), id);
        self.annotation_trees.trees.insert(id, tree);
        id
    }

    /// Returns `to`, a type mapped from annotated type `from`, with `from`'s annotation tree if it
    /// has one (dotty's `derivedAnnotatedType` keeping the annotation).
    pub fn carry_annotation_tree(&mut self, from: TypeId, to: TypeId) -> TypeId {
        match self.annotation_tree(from) {
            Some(tree) if from != to && matches!(self.get(to), Type::Annotated(..)) => {
                self.annotated_with_tree(*self.get(to), tree)
            }
            _ => to,
        }
    }

    /// The annotation tree of annotated type `annotated` read from TASTy, if it has one.
    pub fn annotation_tree(&self, annotated: TypeId) -> Option<AnnotationTree> {
        self.annotation_trees.trees.get(&annotated).copied()
    }
}
