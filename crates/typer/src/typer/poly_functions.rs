// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Polymorphic function types `[T] => List[T] => Option[T]`, values `[T] => (x: T) => e` and
//! their `apply` (dotty's `typedPolyFunction`, `makePolyFunctionType`, `typedPolyFunctionValue`).

use super::{Pt, Typer};
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
        let owner = self.poly_apply_owner(ctx);
        let (tparams, lctx) = match self.poly_type_params(targs, owner, ctx) {
            Ok(entered) => entered,
            Err(refused) => return refused,
        };
        let mut params = Vec::new();
        for (i, a) in self.ast.items(args).to_vec().into_iter().enumerate() {
            let (name, tpt) = match *self.ast.tree(a) {
                Tree::ValDef { name, tpt, .. } => (self.text(name.name).to_owned(), tpt),
                _ => (format!("x${}", i + 1), a),
            };
            if matches!(self.ast.tree(tpt), Tree::ByNameTypeTree { .. }) {
                return self.unsupported(a, "polymorphic function type with a by-name parameter");
            }
            let typed = self.typed_type(tpt, lctx);
            if self.is_error(typed) {
                return typed;
            }
            let p = self.syms.define(self.u, &name, Kind::Param, owner, f::TERM_PARAM.to_term_flags());
            let tpe = self.tpd.tpe(typed);
            self.u.set_info(p, tpe);
            self.levels.enter(lctx.level, &name, p);
            params.push(p);
        }
        let result = self.typed_type(result, lctx);
        if self.is_error(result) {
            return result;
        }
        let binder = self.syms.fresh_binder();
        let names: Vec<(String, TypeId)> =
            params.iter().map(|&p| (self.u.symbol(p).name.clone(), self.u.info(p))).collect();
        let result = self.tpd.tpe(result);
        let result = self.param_refs(result, &params, binder);
        let method = self.u.intern_type(Type::Method {
            binder,
            params: names,
            result,
            implicit: false,
            contextual: false,
        });
        let tpe = self.poly_function_of(&tparams, method);
        self.type_tree(tpe, self.ast.span(id), false)
    }

    /// Polymorphic function value `id` (dotty's `typedPolyFunctionValue`): its function typed with
    /// the type parameters in scope, against the instantiated `apply` of a polymorphic function
    /// type `pt` of the same arities; its method then polymorphic, the closure a `PolyFunction`.
    pub fn typed_poly_function(
        &mut self,
        id: NodeId,
        targs: List,
        body: NodeId,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let fun = self.normalized_poly_body(body);
        let Tree::Function { args, body: fbody, mods: None, .. } = *self.ast.tree(fun) else {
            return self.unsupported(id, "polymorphic function value of another form");
        };
        let owner = self.poly_apply_owner(ctx);
        let (tparams, lctx) = match self.poly_type_params(targs, owner, ctx) {
            Ok(entered) => entered,
            Err(refused) => return refused,
        };
        let fun_pt = match self.instantiated_poly_pt(pt, &tparams, self.ast.items(args).len()) {
            Ok(fun_pt) => fun_pt,
            Err(what) => return self.unsupported(id, what),
        };
        let typed_tparams: Vec<TNodeId> =
            self.ast.items(targs).to_vec().into_iter().map(|p| self.typed_type_def(p, lctx)).collect();
        let block = self.typed_function(fun, args, fbody, fun_pt, lctx);
        let T::Block { stats, expr: closure } = *self.tpd.tree(block) else {
            return if self.is_error(block) {
                block
            } else {
                self.unsupported(id, "polymorphic SAM closure")
            };
        };
        let (&[def], &T::Closure { meth, tpt: None, .. }) = (self.tpd.items(stats), self.tpd.tree(closure))
        else {
            return self.unsupported(id, "polymorphic SAM closure");
        };
        let T::DefDef { name, paramss, tpt, rhs } = *self.tpd.tree(def) else {
            unreachable!("a closure's method")
        };
        let anonfun = self.tpd.sym(def).expect("a closure's method");
        for &p in &tparams {
            self.u.set_owner(p, anonfun);
            self.u.enter(p);
        }
        let method = self.u.info(anonfun);
        let tpe = self.poly_function_of(&tparams, method);
        let Type::Refined(_, _, info) = *self.u.types().get(tpe) else { unreachable!("a refined type") };
        self.u.set_info(anonfun, info);
        let mut clauses = vec![self.tpd.list(&typed_tparams)];
        clauses.extend_from_slice(self.tpd.clause_items(paramss));
        let paramss = self.tpd.clauses(&clauses);
        let span = self.ast.span(id);
        self.tpd.get_mut(def).tree = T::DefDef { name, paramss, tpt, rhs };
        self.tpd.get_mut(def).span = span;
        self.tpd.get_mut(closure).tpe = Some(tpe);
        self.tpd.get_mut(closure).span = span;
        self.tpd.get_mut(meth).span = span.start_pos();
        self.tpd.get_mut(block).tpe = Some(tpe);
        self.tpd.get_mut(block).span = span.to_synthetic();
        block
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
    fn poly_function_of(&mut self, tparams: &[SymbolId], method: TypeId) -> TypeId {
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

    /// The function type a polymorphic function value of `n` parameters is typed against: the
    /// `apply` of polymorphic function type `pt` instantiated to `tparams`, else none.
    fn instantiated_poly_pt(&mut self, pt: Pt, tparams: &[SymbolId], n: usize) -> Result<Pt, &'static str> {
        let Pt::Type(t) = pt else { return Ok(Pt::Any) };
        let dealiased = self.u.dealias(t);
        let Some(info) = self.u.poly_function_apply(dealiased) else { return Ok(Pt::Any) };
        let Type::Poly { binder, params, result } = self.u.types().get(info).clone() else {
            return Ok(Pt::Any);
        };
        let Type::Method { binder: mb, params: formals, result: restpe, .. } =
            self.u.types().get(result).clone()
        else {
            return Ok(Pt::Any);
        };
        if params.len() != tparams.len() || formals.len() != n {
            return Err("polymorphic function value of other arities than its expected type");
        }
        let mut dependent = false;
        self.u.map_type(restpe, &mut |u, x| {
            dependent |= matches!(*u.types().get(x), Type::Param(b, _) if b == mb);
            None
        });
        if dependent {
            return Err("polymorphic function value of a dependent function type");
        }
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        let args: Vec<TypeId> = tparams.iter().map(|&p| self.u.intern_type(Type::Sym(None, p))).collect();
        let mut fun_args: Vec<TypeId> =
            formals.into_iter().map(|(_, p)| self.u.subst(p, binder, &names, &args)).collect();
        fun_args.push(self.u.subst(restpe, binder, &names, &args));
        let Some(class) = self.function_class(n) else { return Err("function of that many parameters") };
        Ok(Pt::Type(self.u.intern_type(Type::Applied(class, fun_args))))
    }

    /// Type parameters `targs` of a polymorphic function owned by `owner`, entered in a scope of
    /// their own, with their bounds; the context of that scope.
    fn poly_type_params(
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
            let info = self.type_def_info(rhs, lctx);
            if matches!(self.u.types().get(info), Type::Error) {
                return Err(self.error_tree(self.ast.span(p)));
            }
            self.u.set_info(s, info);
        }
        Ok((syms, lctx))
    }

    /// A synthetic method outside its owner's declarations, the owner of the parameters of a
    /// polymorphic function until its closure method exists.
    fn poly_apply_owner(&mut self, ctx: Ctx) -> SymbolId {
        let owner = self.syms.define(self.u, "apply", Kind::Def, ctx.owner, f::METHOD.or(f::SYNTHETIC));
        self.u.unenter(owner);
        owner
    }

    /// `body` without enclosing parentheses or empty blocks (dotty's `normalizePolyFunction`).
    fn normalized_poly_body(&self, mut body: NodeId) -> NodeId {
        loop {
            match *self.ast.tree(body) {
                Tree::Parens { t } => body = t,
                Tree::Block { stats, expr: Some(e) } if self.ast.items(stats).is_empty() => body = e,
                _ => return body,
            }
        }
    }
}
