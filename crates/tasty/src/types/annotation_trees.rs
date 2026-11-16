//! The annotation trees of annotated types read from TASTy, which dotty's `AnnotatedType` keeps.

use super::{TypeId, Types};
use crate::{Ast, FileId, NodeId};

/// The annotation term of an annotated type in a TASTy file.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AnnotationTree {
    pub file: FileId,
    pub node: NodeId,
}

impl Types {
    /// Records `tree` of `ast`, the tree of `tree.file`, as the annotation tree of annotated type
    /// `annotated`; a type that another tree gave already, one of another file or of other
    /// tags, names or constants, is marked as given by several.
    pub fn record_annotation_tree(&mut self, annotated: TypeId, tree: AnnotationTree, ast: &Ast) {
        let recorded = self.annotation_trees.entry(annotated).or_insert(Some(tree));
        if let Some(r) = *recorded
            && (r.file != tree.file || !same_tree(ast, r.node, tree.node))
        {
            *recorded = None;
        }
    }

    /// Gives `to`, a type mapped from `from`, the annotation tree of `from` if it has one and the
    /// mapping kept its annotation class.
    pub fn carry_annotation_tree(&mut self, from: TypeId, to: TypeId) {
        let Some(&tree) = self.annotation_trees.get(&from) else { return };
        let recorded = self.annotation_trees.entry(to).or_insert(tree);
        if *recorded != tree {
            *recorded = None;
        }
    }

    /// The annotation tree of annotated type `annotated` read from TASTy: `Some(None)` if several
    /// trees gave it, `None` if none did.
    pub fn annotation_tree(&self, annotated: TypeId) -> Option<Option<AnnotationTree>> {
        self.annotation_trees.get(&annotated).copied()
    }
}

/// Whether the trees at `a` and `b` of `ast` have the same tags, payloads and children.
fn same_tree(ast: &Ast, a: NodeId, b: NodeId) -> bool {
    if a == b {
        return true;
    }
    let (x, y) = (ast.node(a), ast.node(b));
    let (xs, ys) = (ast.kids(a), ast.kids(b));
    x.tag == y.tag
        && (x.a, x.b) == (y.a, y.b)
        && xs.len() == ys.len()
        && xs.iter().zip(ys).all(|(&p, &q)| same_tree(ast, p, q))
}
