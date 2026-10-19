// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Trees.scala
//   compiler/src/dotty/tools/dotc/ast/untpd.scala
//! Whether a tree is a definition, a type or a term (dotty's `isDef`, `isType`, `isTerm`).

use super::{Ast, NodeId, Tree};

impl Ast {
    pub fn is_def(&self, t: NodeId) -> bool {
        matches!(
            self.tree(t),
            Tree::ValDef { .. }
                | Tree::DefDef { .. }
                | Tree::TypeDef { .. }
                | Tree::ModuleDef { .. }
                | Tree::PatDef { .. }
                | Tree::Bind { .. }
        )
    }

    pub fn is_type(&self, t: NodeId) -> bool {
        match self.tree(t) {
            Tree::Ident { name } | Tree::Select { name, .. } => name.is_type,
            Tree::SingletonTypeTree { .. }
            | Tree::RefinedTypeTree { .. }
            | Tree::AppliedTypeTree { .. }
            | Tree::LambdaTypeTree { .. }
            | Tree::TermLambdaTypeTree { .. }
            | Tree::MatchTypeTree { .. }
            | Tree::ByNameTypeTree { .. }
            | Tree::TypeBoundsTree { .. }
            | Tree::TypeTree
            | Tree::DerivedTypeTree { .. }
            | Tree::ContextBounds { .. }
            | Tree::TypedSplice { .. } => true,
            Tree::Function { body, .. } | Tree::PolyFunction { body, .. } => self.is_type(*body),
            Tree::InfixOp { op, .. } | Tree::PostfixOp { op, .. } | Tree::PrefixOp { op, .. } => {
                self.is_type(*op)
            }
            Tree::Parens { t } => self.is_type(*t),
            Tree::Annotated { arg, .. } => self.is_type(*arg),
            Tree::Tuple { trees } => self.items(*trees).first().is_some_and(|&t| {
                let t = match self.tree(t) {
                    Tree::NamedArg { arg, .. } => *arg,
                    _ => t,
                };
                !self.is_term(t)
            }),
            _ => false,
        }
    }

    pub fn is_term(&self, t: NodeId) -> bool {
        match self.tree(t) {
            Tree::Ident { name } | Tree::Select { name, .. } => !name.is_type,
            Tree::Tuple { trees } => self.items(*trees).is_empty() || !self.is_type(t),
            _ => !self.is_type(t) && !self.is_def(t),
        }
    }
}
