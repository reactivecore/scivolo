// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/reporting/messages.scala
//! Identifiers nothing in scope defines (dotty's `MissingIdent`), reported where dotty finds
//! nothing either.

use super::Typer;
use crate::ctx::{Ctx, Mode};
use crate::tpd::TNodeId;
use names::nme;
use rustc_hash::FxHashSet;
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
        if !plain || ctx.mode.is(Mode::PATTERN) || self.u.symbol(ctx.owner).name == nme::CONSTRUCTOR {
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

    /// The outermost definition tree of an owner of `ctx` that contains `id`, else the root of the
    /// code of `typeChecks` if it does.
    pub fn enclosing_definition(&mut self, id: NodeId, ctx: Ctx) -> Option<NodeId> {
        let mut owners = Vec::new();
        let mut owner = Some(ctx.owner);
        while let Some(o) = owner.filter(|&o| self.u.symbol(o).kind != Kind::Package) {
            owners.push(o);
            owner = self.u.symbol(o).owner;
        }
        let defs = self.owner_definitions(&owners);
        let mut module_classes = vec![None; defs.len()];
        for &o in owners.iter().rev() {
            for (&(tree, s), class) in defs.iter().zip(&mut module_classes) {
                if s != o && class.is_none() && self.u.symbol(s).kind == Kind::Val {
                    *class = Some(self.u.module_class(s));
                }
                let defines = s == o || *class == Some(Some(o));
                if defines && self.ast.exists_subtree(tree, &|_, n| n == id) {
                    return Some(tree);
                }
            }
        }
        let (root, _) = self.checked_code?;
        self.ast.exists_subtree(root, &|_, n| n == id).then_some(root)
    }

    /// The definition trees of the unit, with their symbols, that can define one of `owners`: an
    /// owner's own, and those of the values `Universe::module_class` can take to an owner, by the
    /// owner's name or by the value's type.
    fn owner_definitions(&self, owners: &[SymbolId]) -> Vec<(NodeId, SymbolId)> {
        let u = &*self.u;
        let objects: FxHashSet<&str> =
            owners.iter().filter_map(|&o| tasty::module_name(&u.symbol(o).name)).collect();
        let may_define = |s: SymbolId| {
            let sym = u.symbol(s);
            let typed_owner = || u.known_info(s).and_then(|i| u.sym_of(i)).is_some_and(|c| owners.contains(&c));
            owners.contains(&s)
                || sym.kind == Kind::Val
                    && (sym.tree.is_some()
                        || sym.name.starts_with('_')
                        || objects.contains(sym.name.as_str())
                        || typed_owner())
        };
        self.syms.by_tree.iter().filter(|&(_, &s)| may_define(s)).map(|(&t, &s)| (t, s)).collect()
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
