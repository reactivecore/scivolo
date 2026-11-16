//! The annotation trees of annotated types read from TASTy, which dotty's `AnnotatedType` keeps.

use super::{TypeId, Types};
use crate::tags::*;
use crate::{Ast, FileId, NodeId};
use std::fmt::Write;

/// The annotation term of an annotated type in a TASTy file.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AnnotationTree {
    pub file: FileId,
    pub node: NodeId,
}

/// An annotation tree recorded for an annotated type and what tells it from others: its tags,
/// names, constants and, of references to definitions, the file and address.
pub struct Recorded {
    tree: AnnotationTree,
    print: String,
}

impl Types {
    /// Records `tree` of `ast` with names `names`, the trees of `tree.file`, as the annotation
    /// tree of annotated type `annotated`; a type that a tree of other tags, names, constants or
    /// referenced definitions gave already is marked as given by several.
    pub fn record_annotation_tree(
        &mut self,
        annotated: TypeId,
        tree: AnnotationTree,
        (ast, names): (&Ast, &[String]),
    ) {
        let mut print = String::new();
        fingerprint((ast, names), tree, &mut print);
        self.record(annotated, Recorded { tree, print });
    }

    /// Gives `to`, a type mapped from `from`, the annotation tree of `from` if it has one and the
    /// mapping kept its annotation class; marks `to` as given by several if only `to` has one.
    pub fn carry_annotation_tree(&mut self, from: TypeId, to: TypeId) {
        let Some(recorded) = self.annotation_trees.get(&from) else {
            if let Some(recorded) = self.annotation_trees.get_mut(&to) {
                *recorded = None;
            }
            return;
        };
        let recorded = recorded.as_ref().map(|r| Recorded { tree: r.tree, print: r.print.clone() });
        match recorded {
            Some(r) => self.record(to, r),
            None => {
                self.annotation_trees.insert(to, None);
            }
        }
    }

    fn record(&mut self, annotated: TypeId, tree: Recorded) {
        match self.annotation_trees.get_mut(&annotated) {
            None => {
                self.annotation_trees.insert(annotated, Some(tree));
            }
            Some(recorded) => {
                if recorded.as_ref().is_some_and(|r| r.print != tree.print) {
                    *recorded = None;
                }
            }
        }
    }

    /// The annotation tree of annotated type `annotated` read from TASTy: `Some(None)` if
    /// different trees gave it, `None` if none did.
    pub fn annotation_tree(&self, annotated: TypeId) -> Option<Option<AnnotationTree>> {
        self.annotation_trees.get(&annotated).map(|r| r.as_ref().map(|r| r.tree))
    }
}

/// Writes the tags of the tree at `tree` to `out`, shared trees followed, with the names, constants
/// and references to definitions its nodes hold.
fn fingerprint((ast, names): (&Ast, &[String]), tree: AnnotationTree, out: &mut String) {
    let id = ast.deref(tree.node);
    let n = ast.node(id);
    let _ = write!(out, "({}", n.tag);
    match n.tag {
        IDENT | SELECT | SELECTin | IDENTtpt | SELECTtpt | TYPEREF | TERMREF | TYPEREFin | TERMREFin
        | TYPEREFpkg | TERMREFpkg | NAMEDARG | STRINGconst => {
            let _ = write!(out, " {:?}", names.get(n.a as usize));
        }
        TYPEREFsymbol | TERMREFsymbol | TYPEREFdirect | TERMREFdirect | PARAMtype | RECthis => {
            let _ = write!(out, " {:?}@{}", tree.file, n.a);
        }
        _ => {
            let _ = write!(out, " {} {}", n.a, n.b);
        }
    }
    for &k in ast.kids(id) {
        fingerprint((ast, names), AnnotationTree { file: tree.file, node: k }, out);
    }
    out.push(')');
}
