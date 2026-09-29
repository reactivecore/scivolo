//! Tree constructors the parser shares (dotty's `untpd` helpers) and tree predicates.

use super::Parser;
use crate::ast::flags::{self, FlagSet};
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, NodeId, TName, Tree};

impl Parser<'_> {
    /// `(t)` for one tree (unless a named argument), a tuple otherwise.
    pub fn make_tuple_or_parens(&mut self, ts: &[NodeId]) -> NodeId {
        match ts {
            [t] if !matches!(self.ast.tree(*t), Tree::NamedArg { .. }) => {
                self.ast.node(Tree::Parens { t: *t })
            }
            _ => {
                let trees = self.ast.list(ts);
                self.ast.node(Tree::Tuple { trees })
            }
        }
    }

    /// A value parameter `name: tpt` with `mods` and `Param`.
    pub fn make_parameter(&mut self, name: TName, tpt: NodeId, mods: Modifiers, backquoted: bool) -> NodeId {
        let flags = mods.flags.or(flags::PARAM);
        let mods = self.ast.mods(Modifiers { flags, ..mods });
        let v = self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods });
        if backquoted {
            self.ast.add_attrs(v, Attrs::BACKQUOTED);
        }
        v
    }

    pub fn type_tree(&mut self) -> NodeId {
        self.ast.node(Tree::TypeTree)
    }

    pub fn empty_mods(&mut self) -> crate::ast::tree::ModsId {
        self.ast.mods(Modifiers::default())
    }

    pub fn mods_with(&mut self, flags: FlagSet) -> crate::ast::tree::ModsId {
        self.ast.mods(Modifiers::with_flags(flags))
    }

    /// Whether `t` is empty: `EmptyTree` or an untyped `TypeTree()` (dotty's `isEmpty`).
    pub fn is_empty(&self, t: NodeId) -> bool {
        matches!(self.ast.tree(t), Tree::Empty | Tree::TypeTree)
    }

    pub fn empty_tree(&mut self) -> NodeId {
        self.ast.node(Tree::Empty)
    }

    /// Whether `t` is a definition (dotty's `isDef`).
    pub fn is_def(&self, t: NodeId) -> bool {
        matches!(
            self.ast.tree(t),
            Tree::ValDef { .. }
                | Tree::DefDef { .. }
                | Tree::TypeDef { .. }
                | Tree::ModuleDef { .. }
                | Tree::PatDef { .. }
                | Tree::Bind { .. }
        )
    }

    /// Whether `t` is a type tree (dotty's `isType`).
    pub fn is_type(&self, t: NodeId) -> bool {
        match self.ast.tree(t) {
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
            | Tree::ContextBounds { .. }
            | Tree::TypedSplice { .. } => true,
            Tree::Function { body, .. } | Tree::PolyFunction { body, .. } => self.is_type(*body),
            Tree::InfixOp { op, .. } | Tree::PostfixOp { op, .. } | Tree::PrefixOp { op, .. } => {
                self.is_type(*op)
            }
            Tree::Parens { t } => self.is_type(*t),
            Tree::Annotated { arg, .. } => self.is_type(*arg),
            Tree::Tuple { trees } => self.ast.items(*trees).first().is_some_and(|&t| {
                let t = match self.ast.tree(t) {
                    Tree::NamedArg { arg, .. } => *arg,
                    _ => t,
                };
                !self.is_term(t)
            }),
            _ => false,
        }
    }

    /// Whether `t` is a term (dotty's `isTerm`).
    pub fn is_term(&self, t: NodeId) -> bool {
        match self.ast.tree(t) {
            Tree::Ident { name } | Tree::Select { name, .. } => !name.is_type,
            Tree::Tuple { trees } => self.ast.items(*trees).is_empty() || !self.is_type(t),
            _ => !self.is_type(t) && !self.is_def(t),
        }
    }

    /// `Apply(t, Nil)` unless `t` is an application.
    pub fn ensure_applied(&mut self, t: NodeId) -> NodeId {
        if matches!(self.ast.tree(t), Tree::Apply { .. }) {
            return t;
        }
        let args = self.ast.list(&[]);
        self.ast.node(Tree::Apply { fun: t, args })
    }

    /// `Select(New(tpt), <init>)` (dotty's `wrapNew`).
    pub fn wrap_new(&mut self, tpt: NodeId) -> NodeId {
        let new = self.ast.node(Tree::New { tpt });
        let init = self.term("<init>");
        self.ast.node(Tree::Select { qualifier: new, name: init })
    }

    /// The primary constructor `def <init>[tparams](vparamss)` (dotty's `makeConstructor`).
    pub fn make_constructor(&mut self, clauses: &[crate::ast::List], rhs: Option<NodeId>) -> NodeId {
        let name = self.term("<init>");
        let paramss = self.ast.clauses(clauses);
        let tpt = self.type_tree();
        let mods = self.empty_mods();
        self.ast.node(Tree::DefDef { name, paramss, tpt, rhs, mods })
    }

    pub fn empty_constructor(&mut self) -> NodeId {
        self.make_constructor(&[], None)
    }

    /// `A & B` for `A with B` (dotty's `makeAndType`).
    pub fn make_and_type(&mut self, left: NodeId, right: NodeId) -> NodeId {
        let and = self.ast.node(Tree::TypedSplice { what: "&" });
        let args = self.ast.list(&[left, right]);
        self.ast.node(Tree::AppliedTypeTree { tpt: and, args })
    }

    /// Whether `t` is a name of the placeholder parameter most recently introduced by `_`.
    pub fn is_wildcard(&self, t: NodeId) -> bool {
        match self.ast.tree(t) {
            Tree::Ident { name } => self.placeholder_params.last().is_some_and(|&p| match self.ast.tree(p) {
                Tree::ValDef { name: pn, .. } => pn == name,
                _ => false,
            }),
            Tree::Typed { expr, .. } => self.is_wildcard(*expr),
            Tree::Annotated { arg, .. } => self.is_wildcard(*arg),
            Tree::Parens { t } => self.is_wildcard(*t),
            _ => false,
        }
    }

    pub fn is_wildcard_type(&self, t: NodeId) -> bool {
        match self.ast.tree(t) {
            Tree::TypeBoundsTree { .. } => true,
            Tree::Parens { t } => self.is_wildcard_type(*t),
            _ => false,
        }
    }

    /// `t`, or `scala.Any` after an error if `t` is a wildcard type.
    pub fn reject_wildcard_type(&mut self, t: NodeId) -> NodeId {
        if !self.is_wildcard_type(t) {
            return t;
        }
        let span = self.ast.span(t);
        self.syntax_error_span("Unbound wildcard type", span);
        let any = self.type_name("Any");
        self.scala_dot(any)
    }
}
