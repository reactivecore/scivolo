//! Anonymous mirrors (dotty's `anonymousMirror`): `new Object with Serializable { type
//! MirroredMonoType = M }`, marked for the lowering to add the mirror's parent and methods.

use super::super::{Pt, Typer};
use super::mirror_types::MISSING;
use crate::ctx::Ctx;
use crate::tpd::{AnonMirror, TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::TypeId;

impl Typer<'_> {
    /// The anonymous sum (`sum`) or product mirror of `mono` at `at`.
    pub fn anonymous_mirror(
        &mut self,
        mono: TypeId,
        sum: bool,
        at: Span,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let object = crate::defn::type_ref(self.u, self.u.builtins().object);
        let serializable = self.resolve_full_name("java.io.Serializable", true).ok_or(MISSING)?;
        let serializable = crate::defn::type_ref(self.u, serializable);
        let parents = [self.spliced_type(object, at), self.spliced_type(serializable, at)];
        let parents = self.ast.list(&parents);
        let rhs = self.spliced_type(mono, at);
        let name = TName { name: self.name("MirroredMonoType"), is_type: true };
        let mods = self.ast.mods(Modifiers::with_flags(f::EMPTY));
        let mono_def = self.mirror_node(Tree::TypeDef { name, rhs, mods }, at);
        let body = self.ast.list(&[mono_def]);
        let init = TName { name: self.name("<init>"), is_type: false };
        let clause = self.ast.list(&[]);
        let paramss = self.ast.clauses(&[clause]);
        let tpt = self.mirror_node(Tree::TypeTree, at);
        let mods = self.ast.mods(Modifiers::with_flags(f::EMPTY));
        let constr = self.mirror_node(Tree::DefDef { name: init, paramss, tpt, rhs: None, mods }, at);
        let template =
            Tree::Template { constr, parents_or_derived: parents, self_: None, body, derived: None };
        let template = self.mirror_node(template, at);
        let new = self.mirror_node(Tree::New { tpt: template }, at);
        let typed = self.typed(new, Pt::Any, ctx);
        let T::Block { stats, .. } = *self.tpd.tree(typed) else {
            return Err("anonymous Mirror outside the subset");
        };
        let class = self.tpd.items(stats).first().and_then(|&d| self.tpd.sym(d));
        let class = class.ok_or("anonymous Mirror outside the subset")?;
        let kind = if sum { AnonMirror::Sum } else { AnonMirror::Product };
        self.tpd.mark_anon_mirror(class, kind);
        Ok(typed)
    }

    /// An untyped node standing for a type tree of `t` at `at`.
    fn spliced_type(&mut self, t: TypeId, at: Span) -> NodeId {
        let tree = self.type_tree(t, at, false);
        self.spliced(tree)
    }

    /// A new untyped node of `tree` at `at`.
    fn mirror_node(&mut self, tree: Tree, at: Span) -> NodeId {
        let node = self.ast.node(tree);
        self.ast.with_span(node, at);
        node
    }
}
