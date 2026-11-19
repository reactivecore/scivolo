// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! The bodies of export forwarders of a context function type, as closures (dotty's
//! `etaExpandCFT`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use names::{name_kinds, nme};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{Attrs, Span, TName, Tree};

impl Typer<'_> {
    /// Forwarder body `rhs` of a context function type as `(x$0: A, …) ?=> rhs.apply(x$0, …)`
    /// typed in `ctx`, else `rhs` (dotty's `etaExpandCFT`).
    pub fn eta_expand_cft(&mut self, rhs: TNodeId, at: Span, ctx: Ctx) -> TNodeId {
        let tpe = self.tpd.tpe(rhs);
        let tpe = self.u.widen(tpe);
        let Some((formals, _)) = self.context_function_parts(tpe) else { return rhs };
        let Some(&apply) = self.type_members(tpe, nme::APPLY).first() else { return rhs };

        let mut params = Vec::new();
        let mut ids = Vec::new();
        for (i, formal) in formals.into_iter().enumerate() {
            let tpt = self.ast.node(Tree::TypeTree);
            self.ast.with_span(tpt, at);
            let typed = self.type_tree(formal, at, false);
            self.splices.insert(tpt, typed);
            let name = TName { name: self.name(&name_kinds::synthetic_param(i)), is_type: false };
            let mods = self.ast.mods(Modifiers::with_flags(f::GIVEN));
            let param = self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods });
            self.ast.with_span(param, at);
            params.push(param);
            let id = self.ast.node(Tree::Ident { name });
            self.ast.with_span(id, at);
            ids.push(id);
        }

        let selected = self.build().select(rhs, apply, at);
        let fun = self.ast.node(Tree::TypeTree);
        self.ast.with_span(fun, at);
        self.splices.insert(fun, selected);
        self.splice_owners.insert(fun, ctx.owner);
        let args = self.ast.list(&ids);
        let body = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(body, at);
        self.ast.add_attrs(body, Attrs::USING_APPLY);
        let args = self.ast.list(&params);
        let function = self.ast.node(Tree::Function { args, body, mods: None, wildcard: false });
        self.ast.with_span(function, at);

        let closure = self.typed_function(function, args, body, Pt::Type(tpe), ctx);
        if let T::Block { stats, .. } = *self.tpd.tree(closure)
            && let Some(anon) = self.tpd.items(stats).first().and_then(|&d| self.tpd.sym(d))
            && let Some(&flags) = self.syms.flags.get(&anon)
        {
            self.syms.set_flags(self.u, anon, flags.and_not(f::ARTIFACT));
        }
        closure
    }
}
