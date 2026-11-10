// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Type lambdas `[T] =>> body`, the right-hand sides of parameterized type aliases, abstract
//! types and type parameters (dotty's `typedLambdaTypeTree`, `HKTypeLambda.boundsFromParams`).

use super::Typer;
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{List, NodeId, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Type lambda `id` of type parameters `tparams` and `body`, typed in `ctx`: its parameters
    /// entered in a scope of their own, its type a lambda over them, bounds of lambdas for bounds.
    pub fn typed_lambda_type(&mut self, id: NodeId, tparams: List, body: NodeId, ctx: Ctx) -> TNodeId {
        let derived = self.derivations.lambda_owner(id).and_then(|d| self.syms.by_tree.get(&d)).copied();
        let type_def = self.type_def_with_rhs(id).or(derived);
        let owner = type_def.unwrap_or(ctx.owner);
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        let lctx = Ctx { level, owner, ..ctx };
        let params = self.ast.items(tparams).to_vec();
        let mut syms: Vec<SymbolId> = Vec::new();
        for &p in &params {
            let Tree::TypeDef { name, mods, .. } = *self.ast.tree(p) else {
                return self.unsupported(p, "type lambda parameter");
            };
            let flags = self.ast.modifiers(mods).flags;
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
            let Some((lo, hi)) = self.u.bounds(info) else { continue };
            if self.refers_to(lo, &[s]) {
                return self.unsupported(p, "lower bound referring to its type parameter");
            }
            if let Some((tycon, _)) = self.u.applied(hi)
                && self.refers_to(tycon, &syms)
                && self.refers_to(hi, &[s])
            {
                return self.unsupported(p, "bound applying a type lambda parameter to its type parameter");
            }
        }
        let typed_params: Vec<TNodeId> = params.iter().map(|&p| self.typed_type_def(p, lctx)).collect();
        let body = self.typed_type(body, lctx);
        let binder = self.syms.fresh_binder();
        let refs = |u: &mut tasty::Universe, t: TypeId| over_params(u, &syms, binder, t);
        let lambda_params: Vec<(tasty::NameId, TypeId)> = syms
            .iter()
            .map(|&s| {
                let bounds = self.u.info(s);
                (self.u.interned_name(s), refs(self.u, bounds))
            })
            .collect();
        let body_tpe = self.tpd.tpe(body);
        let lambda = |u: &mut tasty::Universe, t: TypeId| {
            let result = refs(u, t);
            u.intern_lambda(binder, &lambda_params, result)
        };
        let bounds = self.u.bounds(body_tpe).is_some();
        let opaque = |s: SymbolId| self.syms.flags.get(&s).is_some_and(|fl| fl.is(f::OPAQUE));
        let match_alias = matches!(self.u.ty(body_tpe), Type::Match { .. });
        let declares = type_def.is_some_and(|s| match_alias || opaque(s));
        if bounds || declares || syms.iter().any(|&s| self.u.has_variance(s)) {
            self.u.declare_variances(binder, &syms);
        }
        if bounds && self.leaks_private_type(owner, body_tpe, &syms) {
            return self.unsupported(id, "private type in the bounds of a higher-kinded type");
        }
        let tpe = match self.u.bounds(body_tpe) {
            Some((lo, hi)) => {
                let nothing = self.u.class_symbol(lo) == Some(self.u.defn().nothing);
                let lo = if nothing { lo } else { lambda(self.u, lo) };
                let hi = lambda(self.u, hi);
                self.u.intern_type(Type::Bounds(lo, hi))
            }
            None => lambda(self.u, body_tpe),
        };
        let tparams = self.tpd.list(&typed_params);
        self.tpd.node(T::LambdaTypeTree { tparams, body }, self.ast.span(id), Some(tpe), None)
    }

    /// Whether `bounds` or the bounds of `params` of non-private `owner` refer to a private type.
    fn leaks_private_type(&mut self, owner: SymbolId, bounds: TypeId, params: &[SymbolId]) -> bool {
        let flags = &self.syms.flags;
        let private = |s: SymbolId| flags.get(&s).is_some_and(|fl| fl.is(f::PRIVATE));
        if private(owner) {
            return false;
        }
        let infos: Vec<TypeId> = params.iter().map(|&p| self.u.info(p)).chain([bounds]).collect();
        let mut leaks = false;
        for t in infos {
            self.u.map_type(t, &mut |u, x| {
                if let Some(s) = u.sym_of(x) {
                    leaks |= u.is_type_symbol(s) && u.symbol(s).kind != Kind::TypeParam && private(s);
                }
                None
            });
        }
        leaks
    }

    /// The type lambda that projection `P#A` of a type alias `A` of a class stands for, as seen
    /// from `P`.
    pub fn projected_alias(&mut self, t: TypeId) -> Option<TypeId> {
        let Some((Some(pre), a)) = self.u.sym_ref(t) else { return None };
        let owner = self.u.symbol(a).owner?;
        self.complete(a);
        let info = self.u.info(a);
        let lambda = match *self.u.ty(info) {
            Type::Alias(lambda) => lambda,
            Type::Lambda { result, .. } if self.u.bounds(result).is_none() => info,
            _ => return None,
        };
        self.u.lambda(lambda).is_some().then(|| self.u.as_seen_from(lambda, pre, owner))
    }

    /// The type definition whose right-hand side is `rhs`: an alias or abstract type being
    /// completed, else a type parameter.
    fn type_def_with_rhs(&self, rhs: NodeId) -> Option<SymbolId> {
        let defines = |tree: NodeId| matches!(*self.ast.tree(tree), Tree::TypeDef { rhs: r, .. } if r == rhs);
        let completing = self.syms.completions.iter().find_map(|(&s, c)| match *c {
            crate::symbols::Completion::Completing(tree) => defines(tree).then_some(s),
            _ => None,
        });
        completing.or_else(|| {
            let (_, &s) = self.syms.by_tree.iter().find(|&(&tree, _)| defines(tree))?;
            (self.u.symbol(s).kind == Kind::TypeParam).then_some(s)
        })
    }
}

/// `t` with references to `params` replaced by the parameters of lambda `binder`.
pub fn over_params(u: &mut tasty::Universe, params: &[SymbolId], binder: tasty::Binder, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, x| {
        let s = u.sym_of(x)?;
        let i = params.iter().position(|&p| p == s)?;
        Some(u.intern_type(Type::Param(binder, i as u32)))
    })
}
