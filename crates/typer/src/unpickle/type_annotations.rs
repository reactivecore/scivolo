// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
//! The annotation trees of a library's annotated types (dotty's `ANNOTATEDtype` case of
//! `readType`), read without positions.

use super::{Refusal, Unpickler, define_repeated};
use crate::tpd::{TNodeId, Tpd};
use syntax::ast::Span;
use syntax::names::Names;
use tasty::tags::*;
use tasty::{AnnotationTree, Ast, NodeId, Universe};

/// Reads annotation tree `tree` of a library's annotated type into a typed tree without
/// positions, as dotty's pickler finds none for it. Refuses a tree that refers to other than
/// packages and their static members, e.g. `@ann(x)` of a parameter `x` or `@tg[A]` of a type
/// parameter `A`, which dotty maps with the type (`Annotation.mapWith`), and trees of other than
/// constructor calls, constants, selections and type trees.
pub fn type_annotation(
    u: &mut Universe,
    tpd: &mut Tpd,
    names: &mut Names,
    tree: AnnotationTree,
) -> Result<TNodeId, Refusal> {
    if u.body_info(tree.file).is_err() {
        return Err("a library annotation of an unreadable TASTy file");
    }
    if !is_static(u.file(tree.file).0, tree.node) {
        return Err("a library type annotation referring to other than static definitions");
    }
    define_repeated(u)?;
    let owner = u.package("scala").ok_or("a library without package scala")?;
    let mut unpickler = Unpickler { u, tpd, names, file: tree.file, meth: owner, owner, holes: None };
    let read = unpickler.term(tree.node)?;
    let mut todo = vec![read];
    while let Some(t) = todo.pop() {
        tpd.get_mut(t).span = Span::NONE;
        todo.extend(tpd.children(t));
    }
    Ok(read)
}

/// Whether the annotation tree at `id` is `new C()`, without value or type arguments.
pub fn is_new_without_arguments(ast: &Ast, id: NodeId) -> bool {
    let apply = ast.deref(id);
    let [select] = *ast.kids(apply) else { return false };
    let select = ast.deref(select);
    let is_new = |&k: &NodeId| ast.tag(ast.deref(k)) == NEW;
    ast.tag(apply) == APPLY && ast.tag(select) == SELECTin && ast.kids(select).first().is_some_and(is_new)
}

/// Whether the tree at `id` refers only to packages and their static members and is of the
/// tags `type_annotation` reads.
fn is_static(ast: &Ast, id: NodeId) -> bool {
    let id = ast.deref(id);
    let static_tag = matches!(
        ast.tag(id),
        APPLY
            | TYPEAPPLY
            | NAMEDARG
            | TYPED
            | NEW
            | IDENT
            | SELECT
            | SELECTin
            | IDENTtpt
            | SELECTtpt
            | APPLIEDtpt
            | APPLIEDtype
            | TYPEREF
            | TERMREF
            | TYPEREFin
            | TERMREFin
            | TYPEREFpkg
            | TERMREFpkg
            | TYPEREFsymbol
            | TERMREFsymbol
            | UNITconst
            | FALSEconst
            | TRUEconst
            | NULLconst
            | BYTEconst
            | SHORTconst
            | CHARconst
            | INTconst
            | LONGconst
            | FLOATconst
            | DOUBLEconst
            | STRINGconst
            | CLASSconst
    );
    static_tag && ast.kids(id).iter().all(|&k| is_static(ast, k))
}
