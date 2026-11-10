// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Type trees: named types, applied types, singleton types (dotty's `typedType`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::{Binding, Kind, Type, TypeId};

impl Typer<'_> {
    pub fn typed_type(&mut self, id: NodeId, ctx: Ctx) -> TNodeId {
        if let Some(t) = self.ready(id, ctx) {
            return t;
        }
        let span = self.ast.span(id);
        match self.ast.tree(id).clone() {
            Tree::Ident { .. } | Tree::Select { .. } | Tree::TypedSplice { .. } => {
                self.typed(id, Pt::Any, ctx)
            }
            Tree::AppliedTypeTree { tpt: op, args } => {
                let items = self.ast.items(args).to_vec();
                if let Some(throws) = self.throws_type(id, op, &items, ctx) {
                    return self.typed_type(throws, ctx);
                }
                let tpt = self.typed_type(op, ctx);
                if self.is_invalid_infix_op(id, op, self.tpd.sym(tpt)) {
                    return self.unsupported(id, "alphanumeric infix type under -source future");
                }
                let tycon = self.tpd.tpe(tpt);
                let mut arg_ids = self.ast.items(args).to_vec();
                let arity = self.type_arity(tycon);
                if !self.is_error(tpt) && arity != arg_ids.len() {
                    let Some(error) = self.wrong_type_arg_count(id, tycon, &arg_ids) else {
                        return self.unsupported(id, "wrong number of type arguments");
                    };
                    if arity == 0 {
                        return tpt;
                    }
                    if arity > arg_ids.len() {
                        arg_ids.iter().for_each(|&a| _ = self.typed_type(a, ctx));
                        return error;
                    }
                    arg_ids.truncate(arity);
                }
                let (mut typed_args, vars): (Vec<TNodeId>, Vec<_>) = arg_ids
                    .iter()
                    .enumerate()
                    .map(|(i, &a)| match self.quoted_type_var(a, ctx) {
                        Some(name) => {
                            let bounds = self.quoted_type_arg_bounds(tycon, i);
                            (self.typed_quoted_type_var(a, name, Pt::Type(bounds)), None)
                        }
                        None => self.typed_pattern_type_arg(a, (tycon, i), ctx),
                    })
                    .unzip();
                let arg_types: Vec<TypeId> = typed_args.iter().map(|&a| self.tpd.tpe(a)).collect();
                if !ctx.mode.is(Mode::CONST_VALUE_ARG)
                    && self
                        .tpd
                        .sym(tpt)
                        .is_some_and(|s| self.u.full_name(s).starts_with("scala.compiletime.ops."))
                    && !self.folds(tycon, &arg_types)
                {
                    return self.unsupported(id, "compile-time operation type");
                }
                if let Some(what) =
                    self.type_args_problem(tycon, arity, &arg_types).filter(|_| !self.is_error(tpt))
                {
                    return self.unsupported(id, what);
                }
                let arg_types: Vec<TypeId> =
                    match typed_args.iter().map(|&a| self.adapted_type_arg(a)).collect() {
                        Ok(types) => types,
                        Err(what) => return self.unsupported(id, what),
                    };
                let and_or = match (self.u.sym_of(tycon), &arg_types[..]) {
                    (Some(s), &[a, b]) => self.and_or_applied(s, a, b),
                    _ => None,
                };
                if and_or.is_some() {
                    for (&a, &t) in arg_ids.iter().zip(&arg_types) {
                        self.check_simple_kinded(t, a);
                    }
                }
                let wildcard =
                    !ctx.mode.is(Mode::PATTERN_TYPE) && arg_types.iter().any(|&a| self.u.bounds(a).is_some());
                let mut reduced = false;
                if wildcard
                    && let Some(s) = self.u.sym_of(tycon)
                    && self.u.symbol(s).kind == Kind::Type
                    && self.syms.flags.contains_key(&s)
                {
                    if !self.reduces_with_wildcards(s) {
                        return self.unsupported(id, "parameterized type alias applied to a wildcard");
                    }
                    reduced = true;
                }
                let arg_types = self.pattern_bound_args(tycon, &mut typed_args, &vars, arg_types, ctx);
                let and_or = match (and_or, self.u.sym_of(tycon), &arg_types[..]) {
                    (Some(_), Some(s), &[a, b]) => self.and_or_applied(s, a, b),
                    _ => None,
                };
                let tpe = and_or.unwrap_or_else(|| self.applied_to(tycon, arg_types));
                let tpe = if reduced { self.bounded_refinements(tpe) } else { tpe };
                let from = self.tpd.sym(tpt).is_some_and(|s| self.u.symbol(s).name == "From");
                let tpe = if from { self.with_case_fields(tpe) } else { tpe };
                if self.is_ground_match(tpe) {
                    return self.unsupported(id, "match type that may reduce");
                }
                let tpe = match ctx.mode.is(Mode::PATTERN_TYPE) {
                    false => self.u.try_normalize(tpe).unwrap_or(tpe),
                    true => tpe,
                };
                let args = self.tpd.list(&typed_args);
                self.tpd.node(T::AppliedTypeTree { tpt, args }, span, Some(tpe), None)
            }
            Tree::SingletonTypeTree { reference } => {
                if let Tree::Annotated { arg, annot } = *self.ast.tree(reference)
                    && ctx.mode.is(Mode::IN_CAPTURE_SET)
                {
                    let singleton = self.ast.node(Tree::SingletonTypeTree { reference: arg });
                    self.ast.with_span(singleton, self.ast.span(arg));
                    return self.typed_annotated_type(id, singleton, annot, ctx);
                }
                let reference = self.typed(reference, Pt::Any, ctx.adding(Mode::SINGLETON_REF));
                if !self.is_stable_path(reference) && !self.is_capability(reference, ctx) {
                    return self.unsupported(id, "singleton type of an unstable reference");
                }
                let tpe = self.tpd.tpe(reference);
                self.tpd.node(T::SingletonTypeTree { reference }, span, Some(tpe), None)
            }
            Tree::Literal { .. } => {
                let reference = self.typed(id, Pt::Any, ctx.with_mode(Mode::NONE));
                let tpe = self.tpd.tpe(reference);
                self.tpd.node(T::SingletonTypeTree { reference }, span, Some(tpe), None)
            }
            Tree::Function { args, body, mods, .. } => self.typed_function_type(id, (args, body), mods, ctx),
            Tree::ByNameTypeTree { result } => self.typed_by_name_type(id, result, ctx),
            Tree::Annotated { arg, annot } => self.typed_annotated_type(id, arg, annot, ctx),
            Tree::TypeBoundsTree { lo, hi, alias } => self.typed_type_bounds(id, lo, hi, alias, ctx),
            Tree::LambdaTypeTree { tparams, body } => self.typed_lambda_type(id, tparams, body, ctx),
            Tree::DerivedTypeTree { kind, watched } => self.typed_derived(id, kind, watched, ctx),
            Tree::ContextBoundTypeTree { .. } => self.typed_context_bound(id, ctx),
            Tree::RefinedTypeTree { tpt, refinements } => self.typed_refined_type(id, tpt, refinements, ctx),
            Tree::PolyFunction { targs, body } => self.typed_poly_function_type(id, targs, body, ctx),
            Tree::Tuple { trees } => self.typed_tuple_type(id, trees, ctx),
            Tree::Parens { t } => self.typed_type(t, ctx),
            Tree::MatchTypeTree { bound, selector, cases } => {
                self.typed_match_type_tree(id, (bound, selector, cases), ctx)
            }
            t => {
                let what = format!("type tree {t:?}");
                self.unsupported(id, what.split([' ', '{']).take(3).collect::<Vec<_>>().join(" ").trim())
            }
        }
    }

    /// Checks that type tree `id` of type `t` is not a type constructor missing its arguments
    /// (dotty's `checkSimpleKinded`).
    pub fn check_simple_kinded(&mut self, t: TypeId, id: NodeId) {
        if self.type_arity(t) > 0 {
            self.unsupported(id, "type constructor without arguments");
        }
    }

    /// `tycon[args]`; an alias that eta-expands a type constructor, `type F[X] = G[X]`, applied
    /// as that constructor (dotty's `appliedTo`).
    pub fn applied_to(&mut self, tycon: TypeId, args: Vec<TypeId>) -> TypeId {
        if let Some(s) = self.u.sym_of(tycon)
            && self.u.symbol(s).kind == Kind::Type
        {
            self.complete(s);
            let info = self.u.info(s);
            let alias = self.u.type_alias(info).unwrap_or(info);
            if let Some(Binding { binder, params, result, .. }) = self.u.lambda(alias)
                && let Some((inner, inner_args)) = self.u.applied(result)
                && inner_args.len() == params.len()
                && args.len() == params.len()
                && inner_args.iter().enumerate().all(|(i, &a)| match self.u.ty(a) {
                    Type::Param(b, k) => *b == binder && *k == i as u32,
                    Type::Local(b, name) => b.0 == binder.0 && *name == params[i].0,
                    _ => false,
                })
            {
                return self.u.intern_applied(inner, &args);
            }
        }
        self.u.intern_applied(tycon, &args)
    }

    /// A type tree of type `tpe` that no source tree gave (dotty's `TypeTree(tpe)`).
    pub fn type_tree(&mut self, tpe: tasty::TypeId, span: syntax::ast::Span, inferred: bool) -> TNodeId {
        self.tpd.node(T::TypeTree { inferred }, span, Some(tpe), None)
    }
}
