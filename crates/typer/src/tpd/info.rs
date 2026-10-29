// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//! Extractors over typed trees (dotty's `TreeInfo`).

use super::{List, TNodeId, Tpd, Tree};
use tasty::{SymbolId, TypeId, Universe};

/// The parts of a class definition's `Template`.
pub struct Template {
    pub constr: TNodeId,
    pub parents: List,
    pub self_: Option<TNodeId>,
    pub body: List,
}

/// The lub of the types of `trees`, of a case its body's (the type of a match or `try`).
pub fn lub_of(tpd: &Tpd, u: &mut Universe, trees: &[TNodeId]) -> TypeId {
    let tpes: Vec<TypeId> = trees
        .iter()
        .map(|&t| match *tpd.tree(t) {
            Tree::CaseDef { body, .. } => tpd.tpe(body),
            _ => tpd.tpe(t),
        })
        .collect();
    let first = tpes.first().copied().unwrap_or(tasty::NOTHING);
    tpes[1..].iter().fold(first, |acc, &t| u.lub(acc, t))
}

impl Tpd {
    /// The template of the class definition `td` (dotty's `ClassDef`/`Template` unapply).
    pub fn class_template(&self, td: TNodeId) -> Option<Template> {
        let Tree::TypeDef { rhs, .. } = *self.tree(td) else { return None };
        let Tree::Template { constr, parents, self_, body } = *self.tree(rhs) else { return None };
        Some(Template { constr, parents, self_, body })
    }

    /// The symbol of the method `t` calls: `meth_part(t)` when it is an `Ident` or `Select`.
    pub fn callee(&self, t: TNodeId) -> Option<SymbolId> {
        let f = self.meth_part(t);
        matches!(self.tree(f), Tree::Ident { .. } | Tree::Select { .. }).then(|| self.sym(f)).flatten()
    }

    /// `f` of `f[T](a)(b)`: `t` without its applications and type applications (dotty's
    /// `methPart`, not looking into blocks).
    pub fn meth_part(&self, mut t: TNodeId) -> TNodeId {
        while let Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } = *self.tree(t) {
            t = fun;
        }
        t
    }

    /// `f[T]` of `f[T](a)(b)`: `t` without its applications (dotty's `stripApply`).
    pub fn strip_apply(&self, mut t: TNodeId) -> TNodeId {
        while let Tree::Apply { fun, .. } = *self.tree(t) {
            t = fun;
        }
        t
    }

    /// Whether `t` is a type tree (dotty's `isType`).
    pub fn is_type_tree(&self, t: TNodeId) -> bool {
        matches!(
            self.tree(t),
            Tree::TypeTree { .. }
                | Tree::SingletonTypeTree { .. }
                | Tree::ByNameTypeTree { .. }
                | Tree::AppliedTypeTree { .. }
                | Tree::TypeBoundsTree { .. }
                | Tree::LambdaTypeTree { .. }
                | Tree::RefinedTypeTree { .. }
                | Tree::Annotated { .. }
        ) || matches!(*self.tree(t), Tree::Ident { name } | Tree::Select { name, .. } if name.is_type)
    }

    /// `e` of `e: T: U` (dotty's `stripTyped`).
    pub fn strip_typed(&self, mut t: TNodeId) -> TNodeId {
        while let Tree::Typed { expr, .. } = *self.tree(t) {
            t = expr;
        }
        t
    }
}
