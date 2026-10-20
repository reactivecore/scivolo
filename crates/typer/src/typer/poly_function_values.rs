// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Polymorphic function values `[T] => (x: T) => e` (dotty's `typedPolyFunctionValue`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Tree};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Polymorphic function value `id`: its function typed with the type parameters in scope,
    /// against the instantiated `apply` of a polymorphic function type `pt` of the same arities;
    /// its method then polymorphic, the closure a `PolyFunction`.
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
        if let (true, Some(&last)) = (self.ast.items(args).is_empty(), typed_tparams.last()) {
            self.tpd.get_mut(tpt).span = self.tpd.span(last).end_pos();
        }
        self.tpd.get_mut(def).tree = T::DefDef { name, paramss, tpt, rhs };
        self.tpd.get_mut(def).span = span;
        self.tpd.get_mut(closure).tpe = Some(tpe);
        self.tpd.get_mut(closure).span = span;
        self.tpd.get_mut(meth).span = span.start_pos();
        self.tpd.get_mut(block).tpe = Some(tpe);
        self.tpd.get_mut(block).span = span.to_synthetic();
        block
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
}
