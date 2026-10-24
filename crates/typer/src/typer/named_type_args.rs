// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! Named type arguments `f[T = A]` (dotty's `typedNamedArgs` and the named case of
//! `assignType` of a `TypeApply`): the parameters not named get type variables.

use super::poly_alternatives::PolyProto;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use syntax::config::Features;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Type application `id` of `fun` to the named arguments `arg_ids`.
    pub fn typed_named_type_apply(
        &mut self,
        id: NodeId,
        fun: NodeId,
        arg_ids: &[NodeId],
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        if !self.feature_enabled(Features::NAMED_TYPE_ARGUMENTS, "experimental.namedTypeArguments", ctx) {
            return self.unsupported(id, "named type arguments without their language import");
        }
        let mut named = Vec::new();
        for &a in arg_ids {
            let Tree::NamedArg { name, arg } = *self.ast.tree(a) else {
                return self.unsupported(id, "named and positional type arguments");
            };
            named.push((self.text(name.name).to_owned(), arg));
        }
        let poly = self.poly_protos.len();
        self.poly_protos.push(PolyProto { args: Vec::new(), ctx, pt, typed: None });
        let f = self.typed(fun, Pt::PolyFun(poly), ctx);
        let f = match self.with_apply(f, Pt::PolyFun(poly), ctx) {
            Ok(f) => f,
            Err(what) => return self.unsupported(id, what),
        };
        let tpe = self.tpd.tpe(f);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let Type::Poly { binder, params, result } = self.u.types().get(info).clone() else {
            return self.unsupported(id, "named type arguments of a non-polymorphic method");
        };
        let span = self.ast.span(id);
        let mut args: Vec<Option<TypeId>> = vec![None; params.len()];
        let mut named_trees = Vec::new();
        for (a, (name, arg)) in arg_ids.iter().zip(named) {
            let Some(i) = params.iter().position(|(n, _)| *n == name) else {
                return self.unsupported(id, "named type argument of no type parameter");
            };
            if args[i].is_some() {
                return self.unsupported(id, "type parameter named twice");
            }
            let typed = self.typed_type(arg, ctx);
            let tpe = match self.adapted_type_arg(typed) {
                Ok(t) => t,
                Err(what) => return self.unsupported(id, what),
            };
            if self.type_arity(tpe) != self.bounds_of(params[i].1).2 {
                return self.unsupported(id, "type argument of another kind");
            }
            args[i] = Some(tpe);
            let name = syntax::ast::TName { name: self.name(&name), is_type: true };
            let node = T::NamedArg { name, arg: typed };
            named_trees.push(self.tpd.node(node, self.ast.span(*a), Some(tpe), None));
        }
        let gaps: Vec<usize> = (0..params.len()).filter(|&i| args[i].is_none()).collect();
        let vars: Vec<TypeId> = gaps.iter().map(|_| self.constraint.fresh(self.u)).collect();
        let types: Vec<TypeId> = args.iter().map(|a| a.unwrap_or(tasty::NOTHING)).collect();
        let types: Vec<TypeId> = (0..params.len())
            .map(|i| gaps.iter().position(|&g| g == i).map_or(types[i], |k| vars[k]))
            .collect();
        let tpe = match self.instantiated_with(f, &types, true) {
            Ok(tpe) => tpe,
            Err(what) => return self.unsupported(id, what),
        };
        let named_list = self.tpd.list(&named_trees);
        if gaps.is_empty() {
            return self.tpd.node(T::TypeApply { fun: f, args: named_list }, span, Some(tpe), None);
        }
        let inner_binder = self.syms.fresh_binder();
        let refs: Vec<TypeId> = (0..params.len())
            .map(|i| match gaps.iter().position(|&g| g == i) {
                Some(k) => self.u.intern_type(Type::Param(inner_binder, k as u32)),
                None => types[i],
            })
            .collect();
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let method = self.tpd.sym(self.tpd.meth_part(f));
        let inner_params = gaps
            .iter()
            .map(|&g| (names[g].clone(), self.instantiate_poly(params[g].1, binder, &names, &refs, method)))
            .collect();
        let inner_result = self.instantiate_poly(result, binder, &names, &refs, method);
        let inner_tpe = self.u.intern_type(Type::Poly {
            binder: inner_binder,
            params: inner_params,
            result: inner_result,
        });
        let inner = self.tpd.node(T::TypeApply { fun: f, args: named_list }, span, Some(inner_tpe), None);
        let gap_trees: Vec<TNodeId> = vars.iter().map(|&v| self.type_tree(v, span, true)).collect();
        let gap_list = self.tpd.list(&gap_trees);
        self.tpd.node(T::TypeApply { fun: inner, args: gap_list }, span.to_synthetic(), Some(tpe), None)
    }
}
