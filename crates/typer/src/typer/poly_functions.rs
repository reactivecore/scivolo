// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Polymorphic function types `[T] => List[T] => Option[T]` and the `apply` of polymorphic
//! functions (dotty's `typedPolyFunction`, `makePolyFunctionType`).

use super::Typer;
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{List, NodeId, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Polymorphic function type `id`: `PolyFunction { def apply[T](x$1: P): R }` (dotty's
    /// `makePolyFunctionType`, typed as a refined type).
    pub fn typed_poly_function_type(&mut self, id: NodeId, targs: List, body: NodeId, ctx: Ctx) -> TNodeId {
        let body = self.normalized_poly_body(body);
        let Tree::Function { args, body: result, mods: None, .. } = *self.ast.tree(body) else {
            return self.unsupported(id, "polymorphic function type of another form");
        };
        let class = self.refinement_class(ctx);
        let owner = self.syms.define(self.u, "apply", Kind::Def, class, f::METHOD.or(f::SYNTHETIC));
        let (tparams, lctx) = match self.poly_type_params(targs, owner, ctx) {
            Ok(entered) => entered,
            Err(refused) => return refused,
        };
        let lctx = Ctx { owner, ..lctx };
        let tparam_trees: Vec<TNodeId> =
            self.ast.items(targs).to_vec().into_iter().map(|p| self.typed_type_def(p, lctx)).collect();
        let (mut params, mut param_trees) = (Vec::new(), Vec::new());
        for (i, a) in self.ast.items(args).to_vec().into_iter().enumerate() {
            let (name, tpt, flags) = match *self.ast.tree(a) {
                Tree::ValDef { name, tpt, .. } => (self.text(name.name).to_owned(), tpt, f::TERM_PARAM),
                _ => (format!("x${}", i + 1), a, f::TERM_PARAM.or(f::SYNTHETIC)),
            };
            if matches!(self.ast.tree(tpt), Tree::ByNameTypeTree { .. }) {
                return self.unsupported(a, "polymorphic function type with a by-name parameter");
            }
            let typed = self.typed_type(tpt, lctx);
            if self.is_error(typed) {
                return typed;
            }
            let p = self.syms.define(self.u, &name, Kind::Param, owner, flags.to_term_flags());
            let tpe = self.tpd.tpe(typed);
            self.u.set_info(p, tpe);
            self.levels.enter(lctx.level, &name, p);
            params.push(p);
            let span = if a == tpt { self.tpd.span(typed).to_synthetic() } else { self.ast.span(a) };
            param_trees.push(self.build().val_def(p, typed, None, span));
        }
        let result_tree = self.typed_type(result, lctx);
        if self.is_error(result_tree) {
            return result_tree;
        }
        let binder = self.syms.fresh_binder();
        let names: Vec<(String, TypeId)> =
            params.iter().map(|&p| (self.u.symbol(p).name.clone(), self.u.info(p))).collect();
        let result = self.tpd.tpe(result_tree);
        let result = self.param_refs(result, &params, binder);
        let method = self.u.intern_type(Type::Method {
            binder,
            params: names,
            result,
            implicit: false,
            contextual: false,
        });
        let tpe = self.poly_function_of(&tparams, method);
        let Type::Refined(parent, _, info) = *self.u.types().get(tpe) else { unreachable!("a refined type") };
        self.u.set_info(owner, info);
        let first = tparam_trees.first().map_or(self.tpd.span(result_tree), |&t| self.tpd.span(t));
        let def_span = first.union(self.tpd.span(result_tree)).to_synthetic();
        let apply = self.build().def_def(owner, &[&tparam_trees, &param_trees], result_tree, None, def_span);
        let tpt = self.type_tree(parent, def_span.start_pos(), false);
        let refinements = self.tpd.list(&[apply]);
        self.tpd.node(T::RefinedTypeTree { tpt, refinements }, self.ast.span(id), Some(tpe), None)
    }

    /// `qual.apply` of a polymorphic function `qual` of type `pre`, typed by the refinement;
    /// `None` for other selections.
    pub fn poly_function_select(
        &mut self,
        id: NodeId,
        qual: TNodeId,
        pre: TypeId,
        name: TName,
    ) -> Option<TNodeId> {
        if name.is_type || self.text(name.name) != "apply" {
            return None;
        }
        self.poly_apply(qual, pre, self.ast.span(id))
    }

    /// The info of `pre.apply` referred to by `fun_tpe` if `pre` is of a polymorphic function type.
    pub fn poly_apply_info(&mut self, fun_tpe: TypeId) -> Option<TypeId> {
        let Type::TermRef(pre, name) = self.u.types().get(fun_tpe).clone() else { return None };
        if name != "apply" {
            return None;
        }
        self.u.poly_function_apply_of(pre)
    }

    /// `qual.apply` at `span` if `qual`'s type `pre` is a polymorphic function type.
    pub fn poly_apply(&mut self, qual: TNodeId, pre: TypeId, span: syntax::ast::Span) -> Option<TNodeId> {
        self.u.poly_function_apply_of(pre)?;
        let tpe = self.u.intern_type(Type::TermRef(pre, "apply".into()));
        let name = TName { name: self.name("apply"), is_type: false };
        Some(self.tpd.node(T::Select { qualifier: qual, name }, span, Some(tpe), None))
    }

    /// `PolyFunction { def apply: [tparams]method }`, `method` referring to `tparams` by symbol.
    pub fn poly_function_of(&mut self, tparams: &[SymbolId], method: TypeId) -> TypeId {
        let binder = self.syms.fresh_binder();
        let params: Vec<(String, TypeId)> = tparams
            .iter()
            .map(|&p| {
                let bounds = self.u.info(p);
                (self.u.symbol(p).name.clone(), self.param_refs(bounds, tparams, binder))
            })
            .collect();
        let result = self.param_refs(method, tparams, binder);
        let info = self.u.intern_type(Type::Poly { binder, params, result });
        let parent = self.u.class_ref("scala.PolyFunction");
        self.u.intern_type(Type::Refined(parent, "apply".into(), info))
    }

    /// Type parameters `targs` of a polymorphic function owned by `owner`, entered in a scope of
    /// their own, with their bounds; the context of that scope.
    pub fn poly_type_params(
        &mut self,
        targs: List,
        owner: SymbolId,
        ctx: Ctx,
    ) -> Result<(Vec<SymbolId>, Ctx), TNodeId> {
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        let lctx = Ctx { level, ..ctx };
        let params = self.ast.items(targs).to_vec();
        let mut syms = Vec::new();
        for &p in &params {
            let Tree::TypeDef { name, mods, rhs } = *self.ast.tree(p) else {
                return Err(self.unsupported(p, "polymorphic function type parameter"));
            };
            if matches!(self.ast.tree(rhs), Tree::ContextBounds { .. }) {
                return Err(self.unsupported(p, "polymorphic function with a context bound"));
            }
            let text = self.text(name.name).to_owned();
            let flags = self.ast.modifiers(mods).flags.or(f::TYPE_PARAM).to_type_flags();
            let s = self.syms.define(self.u, &text, Kind::TypeParam, owner, flags);
            self.syms.by_tree.insert(p, s);
            self.levels.enter(level, &text, s);
            syms.push(s);
        }
        for (&p, &s) in params.iter().zip(&syms) {
            let Tree::TypeDef { rhs, .. } = *self.ast.tree(p) else { continue };
            let info = self.type_def_info(rhs, lctx.with_owner(s));
            if matches!(self.u.types().get(info), Type::Error) {
                return Err(self.error_tree(self.ast.span(p)));
            }
            self.u.set_info(s, info);
        }
        Ok((syms, lctx))
    }

    /// A synthetic method outside its owner's declarations, the owner of the parameters of a
    /// polymorphic function until its closure method exists.
    pub fn poly_apply_owner(&mut self, ctx: Ctx) -> SymbolId {
        let owner = self.syms.define(self.u, "apply", Kind::Def, ctx.owner, f::METHOD.or(f::SYNTHETIC));
        self.u.unenter(owner);
        owner
    }

    /// `body` without enclosing parentheses or empty blocks (dotty's `normalizePolyFunction`).
    pub fn normalized_poly_body(&self, mut body: NodeId) -> NodeId {
        loop {
            match *self.ast.tree(body) {
                Tree::Parens { t } => body = t,
                Tree::Block { stats, expr: Some(e) } if self.ast.items(stats).is_empty() => body = e,
                _ => return body,
            }
        }
    }
}
