//! Type lambdas `[T] =>> body`, the right-hand sides of parameterized type aliases (dotty's
//! `typedLambdaTypeTree`).

use super::Typer;
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{List, NodeId, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Type lambda `id` of type parameters `tparams` and `body`, typed in `ctx`: its parameters
    /// entered in a scope of their own, its type a lambda over them.
    pub fn typed_lambda_type(&mut self, id: NodeId, tparams: List, body: NodeId, ctx: Ctx) -> TNodeId {
        if matches!(self.ast.tree(body), Tree::TypeBoundsTree { .. }) {
            return self.unsupported(id, "higher-kinded abstract type");
        }
        let Some(owner) = self.alias_with_rhs(id) else {
            return self.unsupported(id, "type lambda other than an alias's right-hand side");
        };
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        let lctx = Ctx { level, ..ctx };
        let params = self.ast.items(tparams).to_vec();
        let mut syms: Vec<SymbolId> = Vec::new();
        for &p in &params {
            let Tree::TypeDef { name, mods, rhs } = *self.ast.tree(p) else {
                return self.unsupported(p, "type lambda parameter");
            };
            let flags = self.ast.modifiers(mods).flags;
            if flags.is(f::COVARIANT) || flags.is(f::CONTRAVARIANT) {
                return self.unsupported(p, "type lambda parameter with a variance");
            }
            if matches!(self.ast.tree(rhs), Tree::LambdaTypeTree { .. }) {
                return self.unsupported(p, "higher-kinded type lambda parameter");
            }
            let text = self.text(name.name).to_owned();
            let s = self.syms.define(
                self.u,
                &text,
                Kind::TypeParam,
                owner,
                flags.or(f::TYPE_PARAM).to_type_flags(),
            );
            self.syms.by_tree.insert(p, s);
            self.levels.enter(level, &text, s);
            syms.push(s);
        }
        for (&p, &s) in params.iter().zip(&syms) {
            let Tree::TypeDef { rhs, .. } = *self.ast.tree(p) else { continue };
            let info = self.type_def_info(rhs, lctx);
            self.u.set_info(s, info);
            let mut higher_kinded = false;
            self.u.map_type(info, &mut |u, x| {
                higher_kinded |= matches!(u.types().get(x), Type::Lambda { .. });
                None
            });
            if higher_kinded {
                return self.unsupported(p, "higher-kinded type lambda parameter");
            }
            if let Type::Bounds(lo, _) = *self.u.types().get(info)
                && self.refers_to(lo, &[s])
            {
                return self.unsupported(p, "lower bound referring to its type parameter");
            }
        }
        let typed_params: Vec<TNodeId> = params.iter().map(|&p| self.typed_type_def(p, lctx)).collect();
        let body = self.typed_type(body, lctx);
        let binder = self.syms.fresh_binder();
        let refs = |u: &mut tasty::Universe, t: TypeId| {
            u.map_type(t, &mut |u, x| match *u.types().get(x) {
                Type::Sym(_, s) => {
                    let i = syms.iter().position(|&p| p == s)?;
                    Some(u.intern_type(Type::Param(binder, i as u32)))
                }
                _ => None,
            })
        };
        let lambda_params: Vec<(String, TypeId)> = syms
            .iter()
            .map(|&s| {
                let bounds = self.u.info(s);
                (self.u.symbol(s).name.clone(), refs(self.u, bounds))
            })
            .collect();
        let body_tpe = self.tpd.tpe(body);
        let result = refs(self.u, body_tpe);
        let tpe = self.u.intern_type(Type::Lambda { binder, params: lambda_params, result });
        let tparams = self.tpd.list(&typed_params);
        self.tpd.node(T::LambdaTypeTree { tparams, body }, self.ast.span(id), Some(tpe), None)
    }

    /// The type alias being completed whose right-hand side is `rhs`.
    fn alias_with_rhs(&self, rhs: NodeId) -> Option<SymbolId> {
        self.syms.completions.iter().find_map(|(&s, c)| match *c {
            crate::symbols::Completion::Completing(tree) => {
                matches!(*self.ast.tree(tree), Tree::TypeDef { rhs: r, .. } if r == rhs).then_some(s)
            }
            _ => None,
        })
    }

    /// What about type arguments `args` of a type constructor taking `arity` is outside the
    /// subset: their number, or a class type constructor, which dotty eta-expands to a lambda.
    pub fn type_args_problem(&mut self, arity: usize, args: &[TypeId]) -> Option<&'static str> {
        if arity != args.len() {
            return Some("wrong number of type arguments");
        }
        let constructor = args.iter().any(|&a| {
            matches!(*self.u.types().get(a), Type::Sym(_, s) if self.u.symbol(s).kind == Kind::Class)
                && self.type_arity(a) > 0
        });
        constructor.then_some("class type constructor argument")
    }
}
