//! Synthesized `TypeTest[S, T]` (dotty's `synthesizedTypeTest`): `TypeTest.identity[T]` if `S`
//! conforms to `T`, else `(s: S) => if s.isInstanceOf[T] then Some(s.asInstanceOf[s.type & T])
//! else None`.

use super::super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{SymbolId, TypeId};

const NO_TEST: [&str; 3] = ["scala.AnyVal", "scala.AnyRef", "java.lang.Object"];

impl Typer<'_> {
    pub fn synthesized_type_test(
        &mut self,
        from: TypeId,
        to: TypeId,
        at: Span,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, &'static str> {
        let b = self.u.builtins();
        if self.u.class_symbol(to).is_some_and(|c| c == b.nothing || c == b.null) {
            return Ok(None);
        }
        let from = self.fully_defined_as(from, true)?;
        let to = self.fully_defined_as(to, true)?;
        let to = self.u.normalized(to);
        let class = self.resolve_full_name("scala.reflect.TypeTest", true).ok_or("TypeTest")?;
        let pt = crate::defn::applied(self.u, class, vec![from, to]);
        let at = at.to_synthetic();
        if self.u.is_subtype(from, to) {
            let module = self.resolve_full_name("scala.reflect.TypeTest", false).ok_or("TypeTest")?;
            let qualifier = self.module_splice(module, at);
            let identity = self.ast_select(qualifier, "identity", at);
            let targ = self.type_splice(to, at);
            let tree = self.ast_apply(identity, targ, true, at);
            return Ok(Some(self.typed(tree, Pt::Type(pt), ctx)));
        }
        let symbol = self.u.class_symbol(to).map(|c| self.u.full_name(c));
        if symbol.is_some_and(|n| NO_TEST.contains(&n.as_str())) || !self.is_testable_type(to) {
            return Ok(None);
        }
        let s = TName { name: self.name("s"), is_type: false };
        let tpt = self.type_splice(from, at);
        let mods = self.ast.mods(Modifiers::default());
        let param = self.ast_node(Tree::ValDef { name: s, tpt, rhs: None, mods }, at);
        let reference = self.ast_node(Tree::Ident { name: s }, at);
        let test = self.ast_select(reference, "isInstanceOf", at);
        let targ = self.type_splice(to, at);
        let cond = self.ast_apply(test, targ, true, at);
        let reference = self.ast_node(Tree::Ident { name: s }, at);
        let singleton = self.ast_node(Tree::SingletonTypeTree { reference }, at);
        let and = TName { name: self.name("&"), is_type: true };
        let and = self.ast_node(Tree::Ident { name: and }, at);
        let targ = self.type_splice(to, at);
        let args = self.ast.list(&[singleton, targ]);
        let both = self.ast_node(Tree::AppliedTypeTree { tpt: and, args }, at);
        let reference = self.ast_node(Tree::Ident { name: s }, at);
        let cast = self.ast_select(reference, "asInstanceOf", at);
        let cast = self.ast_apply(cast, both, true, at);
        let some = self.resolve_full_name("scala.Some", false).ok_or("Some")?;
        let some = self.module_splice(some, at);
        let thenp = self.ast_apply(some, cast, false, at);
        let none = self.resolve_full_name("scala.None", false).ok_or("None")?;
        let elsep = Some(self.module_splice(none, at));
        let body = self.ast_node(Tree::If { cond, thenp, elsep, inline: false }, at);
        let args = self.ast.list(&[param]);
        let function = self.ast_node(Tree::Function { args, body, mods: None, wildcard: false }, at);
        Ok(Some(self.typed(function, Pt::Type(pt), ctx)))
    }

    fn ast_node(&mut self, tree: Tree, at: Span) -> NodeId {
        let id = self.ast.node(tree);
        self.ast.with_span(id, at);
        id
    }

    fn ast_select(&mut self, qualifier: NodeId, name: &str, at: Span) -> NodeId {
        let name = TName { name: self.name(name), is_type: false };
        self.ast_node(Tree::Select { qualifier, name }, at)
    }

    /// `fun[arg]` if `types`, else `fun(arg)`.
    fn ast_apply(&mut self, fun: NodeId, arg: NodeId, types: bool, at: Span) -> NodeId {
        let args = self.ast.list(&[arg]);
        let tree = if types { Tree::TypeApply { fun, args } } else { Tree::Apply { fun, args } };
        self.ast_node(tree, at)
    }

    fn type_splice(&mut self, t: TypeId, at: Span) -> NodeId {
        let typed = self.type_tree(t, at, false);
        let id = self.ast_node(Tree::TypeTree, at);
        self.splices.insert(id, typed);
        id
    }

    fn module_splice(&mut self, module: SymbolId, at: Span) -> NodeId {
        let tpe = crate::defn::type_ref(self.u, module);
        let typed = self.build().ident(module, tpe, at);
        let id = self.ast_node(Tree::TypeTree, at);
        self.splices.insert(id, typed);
        id
    }
}
