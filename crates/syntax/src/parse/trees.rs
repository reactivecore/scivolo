//! Tree constructors the parser shares (dotty's `untpd` helpers) and tree predicates.

use super::Parser;
use crate::ast::flags::{self, FlagSet};
use crate::ast::modifiers::Modifiers;
use crate::ast::{Attrs, NodeId, TName, Tree};
use crate::diag::Message;

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

    /// `Apply(t, Nil)` unless `t` is an application.
    pub fn ensure_applied(&mut self, t: NodeId) -> NodeId {
        if matches!(self.ast.tree(t), Tree::Apply { .. }) {
            return t;
        }
        let args = self.ast.list(&[]);
        self.ast.node(Tree::Apply { fun: t, args })
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
        let sym = self.type_name("scala.&");
        let and = self.ast.node(Tree::TypedSplice { sym });
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
        self.syntax_error_span(Message::syntax(35, "Unbound wildcard type"), span);
        let any = self.type_name("Any");
        self.scala_dot(any)
    }
}
