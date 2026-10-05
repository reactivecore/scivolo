//! Type applications: explicit `f[T]` (dotty's `typedTypeApply`), and the type variables a
//! polymorphic function gets when applied without them (`constrained` in `adaptNoArgs`), their
//! bounds checked or added to the constraint.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId};
use tasty::{Binder, SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_type_apply(&mut self, id: NodeId, fun: NodeId, args: List, ctx: Ctx) -> TNodeId {
        let f = self.typed(fun, Pt::PolyFun, ctx);
        let class_of = self.tpd.sym(f).is_some_and(|s| self.u.full_name(s) == "scala.Predef$.classOf");
        let f = match self.with_apply(f, Pt::PolyFun, ctx) {
            Ok(f) => f,
            Err(what) => return self.unsupported(id, what),
        };
        let arg_ids = self.ast.items(args).to_vec();
        let typed_args: Vec<TNodeId> = arg_ids.iter().map(|&a| self.typed_type(a, ctx)).collect();
        let arg_types: Vec<TypeId> = typed_args.iter().map(|&a| self.tpd.tpe(a)).collect();
        let mut tpe = match self.instantiated(f, &arg_types) {
            Ok(tpe) => tpe,
            Err(what) => return self.unsupported(id, what),
        };
        if class_of {
            let [arg] = arg_types[..] else { return self.unsupported(id, "classOf") };
            if !self.is_class_type(arg, true) {
                return self.unsupported(id, "classOf of a type that is not a class");
            }
            tpe = self.u.intern_type(Type::Constant(tasty::tags::CLASSconst, i64::from(arg.0)));
        }
        let args = self.tpd.list(&typed_args);
        let span = self.ast.span(id);
        self.tpd.node(T::TypeApply { fun: f, args }, span, Some(tpe), None)
    }

    /// `f` applied to type arguments if it is polymorphic, else `f`: a constructor of `new C[Ts]`
    /// to `Ts`, any other method to fresh type variables.
    pub fn with_type_vars(&mut self, f: TNodeId) -> Result<TNodeId, &'static str> {
        let tpe = self.tpd.tpe(f);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let Type::Poly { params, .. } = self.u.types().get(info) else { return Ok(f) };
        let n = params.len();
        let span = self.tpd.span(f);
        let (args, trees) = match self.new_type_args(f) {
            Some((args, at)) => {
                let trees: Vec<TNodeId> = args.iter().map(|&a| self.type_tree(a, at, false)).collect();
                (args, trees)
            }
            None => {
                let vars: Vec<TypeId> = (0..n).map(|_| self.constraint.fresh(self.u)).collect();
                let trees = vars.iter().map(|&v| self.type_tree(v, span, true)).collect();
                (vars, trees)
            }
        };
        let tpe = self.instantiated(f, &args)?;
        let trees = self.tpd.list(&trees);
        Ok(self.tpd.node(T::TypeApply { fun: f, args: trees }, span.to_synthetic(), Some(tpe), None))
    }

    /// The type arguments of `C[Ts]` if `f` selects the constructor of `new C[Ts]`, with the
    /// position after the `new` tree.
    fn new_type_args(&mut self, f: TNodeId) -> Option<(Vec<TypeId>, syntax::ast::Span)> {
        let T::Select { qualifier, .. } = *self.tpd.tree(f) else { return None };
        let T::New { .. } = self.tpd.tree(qualifier) else { return None };
        let tpe = self.tpd.tpe(qualifier);
        match self.u.types().get(tpe).clone() {
            Type::Applied(_, args) => Some((args, self.tpd.span(qualifier).end_pos())),
            _ => None,
        }
    }

    /// The type of polymorphic `f` applied to `args`, which must be within the bounds of its
    /// type parameters, or constrained to be if they are type variables.
    pub fn instantiated(&mut self, f: TNodeId, args: &[TypeId]) -> Result<TypeId, &'static str> {
        let fun_tpe = self.tpd.tpe(f);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Type::Poly { binder, params, result } = self.u.types().get(info).clone() else {
            return Err("type application of a non-polymorphic method");
        };
        if args.len() != params.len() {
            return Err("wrong number of type arguments");
        }
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let method = self.tpd.sym(f);
        for (&(_, bounds), &arg) in params.iter().zip(args) {
            let bounds = self.instantiate_poly(bounds, binder, &names, args, method);
            let (lo, hi) = match *self.u.types().get(bounds) {
                Type::Bounds(lo, hi) => (lo, hi),
                _ => (bounds, bounds),
            };
            if self.type_arity(arg) != self.type_arity(hi) {
                return Err("type argument of another kind");
            }
            if !self.conforms(lo, arg) || !self.conforms(arg, hi) {
                return Err("type argument outside its bounds");
            }
        }
        Ok(self.instantiate_poly(result, binder, &names, args, method))
    }

    /// `t` with the type parameters of binder `binder`, named `names`, and the type parameter
    /// symbols of `method` replaced by `args`.
    pub fn instantiate_poly(
        &mut self,
        t: TypeId,
        binder: Binder,
        names: &[String],
        args: &[TypeId],
        method: Option<SymbolId>,
    ) -> TypeId {
        let t = self.u.subst(t, binder, names, args);
        self.u.map_type(t, &mut |u, part| match *u.types().get(part) {
            Type::Sym(_, s) if method.is_some() && u.symbol(s).owner == method => {
                let name = &u.symbol(s).name;
                names.iter().position(|n| n == name).map(|i| args[i])
            }
            _ => None,
        })
    }
}
