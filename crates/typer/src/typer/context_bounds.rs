// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Context bound type trees, `T: C` as the evidence's type `C[T]` (dotty's
//! `typedContextBoundTypeTree`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use syntax::config::Features;
use tasty::{Kind, TypeId};

/// The type member a type class without type parameters names its instance type by.
const SELF: &str = "Self";

impl Typer<'_> {
    pub fn typed_context_bound(&mut self, id: NodeId, ctx: Ctx) -> TNodeId {
        let Tree::ContextBoundTypeTree { tycon, param_name, .. } = *self.ast.tree(id) else {
            unreachable!("a context bound")
        };
        let typed_tycon = self.typed_type(tycon, ctx);
        let tycon_tpe = self.tpd.tpe(typed_tycon);
        let span = self.ast.span(id);
        let splice = self.ast.node(Tree::TypeTree);
        self.ast.with_span(splice, self.tpd.span(typed_tycon));
        self.splices.insert(splice, typed_tycon);
        let tparam = self.ast.node(Tree::Ident { name: param_name });
        self.ast.with_span(tparam, span.with_end(span.point()));

        if self.type_arity(tycon_tpe) > 0 {
            let args = self.ast.list(&[tparam]);
            let applied = self.ast.node(Tree::AppliedTypeTree { tpt: splice, args });
            let envelope = self.ast.span(splice).union(self.ast.span(tparam));
            self.ast.with_span(applied, envelope.to_synthetic());
            return self.typed_type(applied, ctx);
        }
        if !self.feature_enabled(Features::MODULARITY, "experimental.modularity", ctx)
            || !self.has_abstract_self(tycon_tpe)
        {
            return self.unsupported(id, "context bound of a type without parameters");
        }
        let envelope = self.ast.span(splice).union(self.ast.span(tparam));
        self.self_refined(splice, tparam, envelope, ctx)
    }

    /// Whether type class `t` has an abstract type member `Self`.
    fn has_abstract_self(&mut self, t: TypeId) -> bool {
        let selfs = self.type_members(t, SELF);
        selfs.into_iter().any(|s| match self.u.symbol(s).kind {
            Kind::TypeParam => true,
            Kind::Type => self.u.alias_of(s).is_none() && !self.u.is_class(s),
            _ => false,
        })
    }

    /// `C { type Self = T }` for the context bound `T: C` at `span`, `C` spliced as `tycon`.
    fn self_refined(&mut self, tycon: NodeId, tparam: NodeId, span: Span, ctx: Ctx) -> TNodeId {
        let typed_param = self.typed_type(tparam, ctx);
        let rhs = self.ast.node(Tree::TypeTree);
        self.ast.with_span(rhs, self.tpd.span(typed_param));
        self.splices.insert(rhs, typed_param);
        let name = TName { name: self.name(SELF), is_type: true };
        let mods = self.ast.mods(Modifiers::default());
        let alias = self.ast.node(Tree::TypeDef { name, rhs, mods });
        self.ast.with_span(alias, span.to_synthetic());
        let refinements = self.ast.list(&[alias]);
        let refined = self.ast.node(Tree::RefinedTypeTree { tpt: Some(tycon), refinements });
        self.ast.with_span(refined, span.to_synthetic());
        self.typed_type(refined, ctx)
    }
}
