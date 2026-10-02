//! Type trees: named types, applied types, singleton types (dotty's `typedType`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    pub fn typed_type(&mut self, id: NodeId, ctx: Ctx) -> TNodeId {
        if let Some(t) = self.ready(id) {
            return t;
        }
        let span = self.ast.span(id);
        match self.ast.tree(id).clone() {
            Tree::Ident { .. } | Tree::Select { .. } | Tree::TypedSplice { .. } => {
                self.typed(id, Pt::Any, ctx)
            }
            Tree::AppliedTypeTree { tpt, args } => {
                let tpt = self.typed_type(tpt, ctx);
                let arg_ids = self.ast.items(args).to_vec();
                let typed_args: Vec<TNodeId> = arg_ids.iter().map(|&a| self.typed_type(a, ctx)).collect();
                let arg_types = typed_args.iter().map(|&a| self.tpd.tpe(a)).collect();
                let tycon = self.tpd.tpe(tpt);
                if self.type_arity(tycon) != typed_args.len() {
                    return self.unsupported(id, "wrong number of type arguments");
                }
                let tpe = self.applied_to(tycon, arg_types);
                let args = self.tpd.list(&typed_args);
                self.tpd.node(T::AppliedTypeTree { tpt, args }, span, Some(tpe), None)
            }
            Tree::SingletonTypeTree { reference } => {
                let reference = self.typed(reference, Pt::Any, ctx);
                let tpe = self.tpd.tpe(reference);
                self.tpd.node(T::SingletonTypeTree { reference }, span, Some(tpe), None)
            }
            Tree::Annotated { arg, annot } => {
                let arg = self.typed_type(arg, ctx);
                let annot = self.typed(annot, Pt::Any, ctx);
                let annot_tpe = self.tpd.tpe(annot);
                if !self.is_repeated_annot(annot_tpe) {
                    return self.unsupported(id, "annotated type");
                }
                let tpe = self.u.intern_type(Type::Annotated(self.tpd.tpe(arg), annot_tpe));
                self.tpd.node(T::Annotated { arg, annot }, span, Some(tpe), None)
            }
            Tree::TypeBoundsTree { lo, hi, alias } => self.typed_type_bounds(id, lo, hi, alias, ctx),
            Tree::DerivedTypeTree { kind, watched } => self.typed_derived(id, kind, watched, ctx),
            t => {
                let what = format!("type tree {t:?}");
                self.unsupported(id, what.split([' ', '{']).take(3).collect::<Vec<_>>().join(" ").trim())
            }
        }
    }

    /// How many type parameters type constructor `t` takes: a class's, a type lambda's.
    pub fn type_arity(&mut self, t: TypeId) -> usize {
        self.type_arity_within(t, 32)
    }

    fn type_arity_within(&mut self, t: TypeId, depth: u32) -> usize {
        let Some(depth) = depth.checked_sub(1) else { return 0 };
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) => match self.u.symbol(s).kind {
                Kind::Class => self.u.type_params(s).len(),
                Kind::Type | Kind::TypeParam => {
                    self.complete(s);
                    let info = self.u.info(s);
                    match self.u.types().get(info).clone() {
                        Type::Alias(a) | Type::Bounds(_, a) => self.type_arity_within(a, depth),
                        Type::Lambda { params, .. } => params.len(),
                        _ => 0,
                    }
                }
                _ => 0,
            },
            Type::Lambda { params, .. } => params.len(),
            _ => 0,
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
        if let Type::Sym(_, s) = *self.u.types().get(tycon)
            && self.u.symbol(s).kind == Kind::Type
        {
            self.complete(s);
            let info = self.u.info(s);
            let alias = match *self.u.types().get(info) {
                Type::Alias(a) => a,
                _ => info,
            };
            if let Type::Lambda { binder, ref params, result } = *self.u.types().get(alias)
                && let Type::Applied(inner, ref inner_args) = *self.u.types().get(result)
                && inner_args.len() == params.len()
                && args.len() == params.len()
                && inner_args.iter().enumerate().all(|(i, &a)| match self.u.types().get(a) {
                    Type::Param(b, k) => *b == binder && *k == i as u32,
                    Type::Local(b, name) => b.0 == binder.0 && *name == params[i].0,
                    _ => false,
                })
            {
                return self.u.intern_type(Type::Applied(inner, args));
            }
        }
        self.u.intern_type(Type::Applied(tycon, args))
    }

    /// A type tree of type `tpe` that no source tree gave (dotty's `TypeTree(tpe)`).
    pub fn type_tree(&mut self, tpe: tasty::TypeId, span: syntax::ast::Span, inferred: bool) -> TNodeId {
        self.tpd.node(T::TypeTree { inferred }, span, Some(tpe), None)
    }
}
