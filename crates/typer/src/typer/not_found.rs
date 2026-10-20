// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/reporting/messages.scala
//! Identifiers nothing in scope defines (dotty's `MissingIdent`), reported where dotty finds
//! nothing either.

use super::Typer;
use crate::ctx::{Ctx, Mode};
use crate::tpd::TNodeId;
use syntax::ast::children::Child;
use syntax::ast::{Ast, NodeId, TName, Tree};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// `MissingIdent` for identifier `id` named `name` that `find_ref` finds nowhere in `ctx`, an
    /// error tree; `None` where dotty may find it, words the message otherwise, suggests a name, or
    /// types trees that typing skips after the error.
    pub fn not_found(&mut self, id: NodeId, name: TName, ctx: Ctx) -> Option<TNodeId> {
        let text = self.text(name.name).to_owned();
        let plain = text.chars().next().is_some_and(|c| !c.is_ascii_digit())
            && text.chars().all(|c| c.is_alphanumeric() || c == '_');
        if !plain || ctx.mode.is(Mode::PATTERN) || self.u.symbol(ctx.owner).name == "<init>" {
            return None;
        }
        let root = self.enclosing_definition(id, ctx)?;
        let mut path = Vec::new();
        if !path_to(self.ast, root, id, &mut path) || path.windows(2).any(|w| uncertain_edge(self.ast, w)) {
            return None;
        }
        let message = self.missing_ident(&text, name.is_type, ctx)?;
        Some(self.error(id, message))
    }

    /// The outermost definition tree of an owner of `ctx` that contains `id`.
    pub fn enclosing_definition(&mut self, id: NodeId, ctx: Ctx) -> Option<NodeId> {
        let mut owners = Vec::new();
        let mut owner = Some(ctx.owner);
        while let Some(o) = owner.filter(|&o| self.u.symbol(o).kind != Kind::Package) {
            owners.push(o);
            owner = self.u.symbol(o).owner;
        }
        let defs: Vec<(NodeId, SymbolId)> = self.syms.by_tree.iter().map(|(&t, &s)| (t, s)).collect();
        for &o in owners.iter().rev() {
            for &(tree, s) in &defs {
                let defines =
                    s == o || self.u.symbol(s).kind == Kind::Val && self.u.module_class(s) == Some(o);
                if defines && self.ast.exists_subtree(tree, &|_, n| n == id) {
                    return Some(tree);
                }
            }
        }
        None
    }
}

/// Whether the edge from tree `w[0]` to its child `w[1]` leads into a pattern, the function part
/// of an application, an operand or an assigned variable.
pub fn uncertain_edge(ast: &Ast, w: &[NodeId]) -> bool {
    let child = w[1];
    match *ast.tree(w[0]) {
        Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } => fun == child,
        Tree::Assign { lhs, .. } => lhs == child,
        Tree::CaseDef { pat, .. } | Tree::GenFrom { pat, .. } | Tree::GenAlias { pat, .. } => pat == child,
        Tree::InfixOp { .. } | Tree::PostfixOp { .. } | Tree::PrefixOp { .. } => true,
        Tree::PatDef { .. } | Tree::Bind { .. } | Tree::Alternative { .. } => true,
        _ => false,
    }
}

/// Whether `to` is `from` or under it; the trees from `from` to `to` pushed onto `path` if so.
pub fn path_to(ast: &Ast, from: NodeId, to: NodeId, path: &mut Vec<NodeId>) -> bool {
    path.push(from);
    if from == to {
        return true;
    }
    let found = ast.children(from).iter().any(|&c| match c {
        Child::Node(n) => path_to(ast, n, to, path),
        Child::List(l) => ast.items(l).iter().any(|&n| path_to(ast, n, to, path)),
        Child::Clauses(cs) => {
            ast.clause_items(cs).iter().any(|&l| ast.items(l).iter().any(|&n| path_to(ast, n, to, path)))
        }
        Child::Empty | Child::Mods(_) => false,
    });
    if !found {
        path.pop();
    }
    found
}
