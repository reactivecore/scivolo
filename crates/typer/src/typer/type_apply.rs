// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Type applications: explicit `f[T]` (dotty's `typedTypeApply`), and the type variables a
//! polymorphic function gets when applied without them (`constrained` in `adaptNoArgs`), their
//! bounds checked or added to the constraint.

use super::poly_alternatives::PolyProto;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId};
use tasty::{Binder, SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    pub fn typed_type_apply(&mut self, id: NodeId, fun: NodeId, args: List, pt: Pt, ctx: Ctx) -> TNodeId {
        let poly = self.poly_protos.len();
        let arg_ids = self.ast.items(args).to_vec();
        self.poly_protos.push(PolyProto { args: arg_ids.clone(), ctx, pt, typed: None });
        let f = self.typed(fun, Pt::PolyFun(poly), ctx);
        let fun_name = self.tpd.sym(f).map(|s| self.u.full_name(s));
        let class_of = fun_name.as_deref() == Some("scala.Predef$.classOf");
        let f = match self.with_apply(f, Pt::PolyFun(poly), ctx) {
            Ok(f) => f,
            Err(what) => return self.unsupported(id, what),
        };
        if self.is_pending_dynamic(f) {
            return self.dynamic_type_apply(f, arg_ids, pt, ctx);
        }
        let const_value = matches!(
            fun_name.as_deref().and_then(|n| n.strip_prefix("scala.compiletime.package$package$.")),
            Some("constValue" | "constValueOpt")
        );
        let arg_ctx = if const_value { ctx.with_mode(ctx.mode.with(Mode::CONST_VALUE_ARG)) } else { ctx };
        let typed_args: Vec<TNodeId> = match self.poly_protos[poly].typed.take() {
            Some(typed) => typed,
            None => arg_ids.iter().map(|&a| self.typed_type(a, arg_ctx)).collect(),
        };
        let arg_types = match typed_args.iter().map(|&a| self.adapted_type_arg(a)).collect() {
            Ok::<Vec<TypeId>, _>(types) => types,
            Err(what) => return self.unsupported(id, what),
        };
        let mut tpe = match self.instantiated(f, &arg_types) {
            Ok(tpe) => tpe,
            Err(what) => return self.unsupported(id, what),
        };
        if class_of {
            let [arg] = arg_types[..] else { return self.unsupported(id, "classOf") };
            let module = match self.u.types().get(arg) {
                Type::Sym(_, s) => self.u.symbol(*s).flags.has(tasty::tags::OBJECT),
                _ => false,
            };
            if !module && !self.is_class_type(arg, true) {
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
        let (args, trees, fresh) = match self.new_type_args(f) {
            Some((args, at)) => {
                let trees: Vec<TNodeId> = args.iter().map(|&a| self.type_tree(a, at, false)).collect();
                (args, trees, false)
            }
            None => {
                let vars: Vec<TypeId> = (0..n).map(|_| self.constraint.fresh(self.u)).collect();
                let trees = vars.iter().map(|&v| self.type_tree(v, span, true)).collect();
                (vars, trees, true)
            }
        };
        let tpe = self.instantiated_with(f, &args, fresh)?;
        let trees = self.tpd.list(&trees);
        Ok(self.tpd.node(T::TypeApply { fun: f, args: trees }, span.to_synthetic(), Some(tpe), None))
    }

    /// The type arguments of `C[Ts]` if `f` selects the constructor of `new C[Ts]`, with the
    /// position after the `new` tree, or the class's own type parameters if `f` selects the
    /// constructor of `this` (dotty's `argTypesLo` of a constructor's qualifier).
    fn new_type_args(&mut self, f: TNodeId) -> Option<(Vec<TypeId>, syntax::ast::Span)> {
        let T::Select { qualifier, .. } = *self.tpd.tree(f) else { return None };
        let tpe = match self.tpd.tree(qualifier) {
            T::New { .. } => self.tpd.tpe(qualifier),
            T::This { .. } if self.tpd.sym(f).is_some_and(|s| self.u.is_constructor(s)) => {
                let this = self.tpd.tpe(qualifier);
                self.u.widen(this)
            }
            _ => return None,
        };
        match self.u.types().get(tpe).clone() {
            Type::Applied(_, args) => Some((args, self.tpd.span(qualifier).end_pos())),
            _ => None,
        }
    }

    /// The type of polymorphic `f` applied to `args`, which must be within the bounds of its
    /// type parameters, or constrained to be if they are type variables.
    pub fn instantiated(&mut self, f: TNodeId, args: &[TypeId]) -> Result<TypeId, &'static str> {
        self.instantiated_with(f, args, false)
    }

    /// `instantiated`, without the kind check if `args` are fresh type variables.
    fn instantiated_with(
        &mut self,
        f: TNodeId,
        args: &[TypeId],
        fresh: bool,
    ) -> Result<TypeId, &'static str> {
        let fun_tpe = self.tpd.tpe(f);
        let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
        let Type::Poly { binder, params, result } = self.u.types().get(info).clone() else {
            return Err("type application of a non-polymorphic method");
        };
        if args.len() != params.len() {
            return Err("wrong number of type arguments");
        }
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let method = self.tpd.sym(self.tpd.meth_part(f));
        for (&(_, bounds), &arg) in params.iter().zip(args) {
            let bounds = self.instantiate_poly(bounds, binder, &names, args, method);
            let (lo, hi, arity) = self.bounds_of(bounds);
            if !fresh && self.type_arity(arg) != arity {
                return Err("type argument of another kind");
            }
            let var = match *self.u.types().get(arg) {
                Type::Var(v) if fresh => Some(v),
                _ => None,
            };
            let upper = match var {
                Some(v) if self.u.dealias(hi) != hi && self.open_vars(hi).is_empty() => {
                    self.constraint.add_upper(self.u, v, hi)
                }
                _ => self.conforms(arg, hi),
            };
            if !upper || lo.is_some_and(|lo| !self.conforms(lo, arg)) {
                return Err("type argument outside its bounds");
            }
        }
        Ok(self.instantiate_poly(result, binder, &names, args, method))
    }

    /// The lower and upper bound of type parameter info `bounds` and the arity of the parameter;
    /// of a higher-kinded one, `[X] =>> >: L <: H`, `[X] =>> L` and `[X] =>> H`, the lower one
    /// `None` for `Nothing`.
    pub fn bounds_of(&mut self, bounds: TypeId) -> (Option<TypeId>, TypeId, usize) {
        match self.u.types().get(bounds).clone() {
            Type::Bounds(lo, hi) => (Some(lo), hi, self.type_arity(hi)),
            Type::Lambda { binder, params, result } => {
                let (lo, hi) = match *self.u.types().get(result) {
                    Type::Bounds(lo, hi) => (lo, hi),
                    _ => (result, result),
                };
                let bottom = self.u.class_symbol(lo) == Some(self.u.builtins().nothing);
                let arity = params.len();
                let mut lambda = |result| {
                    let params = params.clone();
                    self.u.intern_type(Type::Lambda { binder, params, result })
                };
                let lo = lambda(lo);
                ((!bottom).then_some(lo), lambda(hi), arity)
            }
            _ => (Some(bounds), bounds, self.type_arity(bounds)),
        }
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
        instantiate_poly(self.u, t, (binder, names), args, method)
    }
}

/// `t` with the type parameters of binder `binder`, named `names`, and the type parameter
/// symbols of `method` replaced by `args`.
pub fn instantiate_poly(
    u: &mut tasty::Universe,
    t: TypeId,
    (binder, names): (Binder, &[String]),
    args: &[TypeId],
    method: Option<SymbolId>,
) -> TypeId {
    let t = u.subst(t, binder, names, args);
    u.map_type(t, &mut |u, part| match *u.types().get(part) {
        Type::Sym(_, s) if method.is_some() && u.symbol(s).owner == method => {
            let name = &u.symbol(s).name;
            names.iter().position(|n| n == name).map(|i| args[i])
        }
        _ => None,
    })
}
