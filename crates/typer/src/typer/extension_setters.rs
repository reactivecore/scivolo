// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Assignments to extension getters, `q.x = v` as `x_=(q)(v)` (the extension case of dotty's
//! `typedAssign`).

use super::paths::MAX_PATH_DEPTH;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Attrs, NodeId, Span, TName, Tree};
use tasty::Type;

impl Typer<'_> {
    /// `lhs = rhs` of assignment `id` as a call of the setter of extension getter application
    /// `lhs`, typed from `lhs_tree`; `None` if `lhs` applies no extension method.
    pub fn extension_setter_call(
        &mut self,
        id: NodeId,
        (lhs, lhs_tree): (TNodeId, NodeId),
        rhs: NodeId,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        if !matches!(self.tpd.tree(lhs), T::Apply { .. }) || self.is_error(lhs) {
            return None;
        }
        let setter = self.extension_setter(lhs, ctx)?;
        let args = self.ast.list(&[rhs]);
        let call = self.ast.node(Tree::Apply { fun: setter, args });
        let span = Span::synthetic(self.ast.span(lhs_tree).start(), self.ast.span(rhs).end());
        self.ast.with_span(call, span);
        let getter_owner = self.called_owner(lhs);
        let typed = self.typed_apply(call, setter, args, Pt::Any, ctx);
        if self.called_owner(typed) != getter_owner {
            return Some(self.unsupported(id, "extension setter of another owner"));
        }
        Some(typed)
    }

    /// Typed extension getter application `tree` with the getter replaced by its setter, the
    /// arguments spliced, if `tree` applies an extension method (dotty's `toSetter`).
    fn extension_setter(&mut self, tree: TNodeId, ctx: Ctx) -> Option<NodeId> {
        let node = match *self.tpd.tree(tree) {
            T::Apply { fun, args } | T::TypeApply { fun, args } => {
                let setter = self.extension_setter(fun, ctx)?;
                let items: Vec<NodeId> =
                    self.tpd.items(args).to_vec().into_iter().map(|a| self.spliced(a)).collect();
                let args = self.ast.list(&items);
                match self.tpd.tree(tree) {
                    T::Apply { .. } => Tree::Apply { fun: setter, args },
                    _ => Tree::TypeApply { fun: setter, args },
                }
            }
            T::Ident { name } | T::Select { name, .. } => {
                let getter = self.tpd.sym(tree)?;
                if !self.u.symbol(getter).flags.has(tasty::tags::EXTENSION) {
                    return None;
                }
                let name = TName { name: self.name(&format!("{}_=", self.text(name.name))), is_type: false };
                let prefix = match *self.u.types().get(self.tpd.tpe(tree)) {
                    Type::Sym(Some(p), _) if !matches!(self.u.types().get(p), Type::NoPrefix) => Some(p),
                    _ => None,
                };
                match (self.tpd.tree(tree).clone(), prefix) {
                    (T::Select { qualifier, .. }, _) => {
                        Tree::Select { qualifier: self.spliced(qualifier), name }
                    }
                    (_, Some(p)) => {
                        let span = self.tpd.span(tree).start_pos();
                        let qualifier = self.path_tree_from(p, span, MAX_PATH_DEPTH, Some(ctx))?;
                        Tree::Select { qualifier: self.spliced(qualifier), name }
                    }
                    _ => Tree::Ident { name },
                }
            }
            _ => return None,
        };
        let using = match *self.tpd.tree(tree) {
            T::Apply { fun, .. } => matches!(self.tpd.tree(fun), T::Apply { .. }),
            _ => false,
        };
        let node = self.ast.node(node);
        self.ast.with_span(node, self.tpd.span(tree));
        if using {
            self.ast.add_attrs(node, Attrs::USING_APPLY);
        }
        Some(node)
    }
}
