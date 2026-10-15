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
        let Some(owner) = self.type_def_with_rhs(id) else {
            return self.unsupported(id, "type lambda other than the right-hand side of a type definition");
        };
        if matches!(self.ast.tree(body), Tree::TypeBoundsTree { alias: Some(_), .. }) {
            return self.unsupported(id, "opaque type");
        }
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
        let lambda = |u: &mut tasty::Universe, t: TypeId| {
            let result = refs(u, t);
            u.intern_type(Type::Lambda { binder, params: lambda_params.clone(), result })
        };
        let bounds = matches!(self.u.types().get(body_tpe), Type::Bounds(..));
        if bounds && self.leaks_private_type(owner, body_tpe, &syms) {
            return self.unsupported(id, "private type in the bounds of a higher-kinded type");
        }
        if !bounds && self.overrides_type_member(owner) {
            return self.unsupported(id, "higher-kinded type alias overriding a type member");
        }
        let tpe = match *self.u.types().get(body_tpe) {
            Type::Bounds(lo, hi) => {
                let nothing = self.u.class_symbol(lo) == Some(self.u.builtins().nothing);
                let lo = if nothing { lo } else { lambda(self.u, lo) };
                let hi = lambda(self.u, hi);
                self.u.intern_type(Type::Bounds(lo, hi))
            }
            _ => lambda(self.u, body_tpe),
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
                if let Type::Sym(_, s) = *u.types().get(x) {
                    leaks |= u.is_type_symbol(s) && u.symbol(s).kind != Kind::TypeParam && private(s);
                }
                None
            });
        }
        leaks
    }

    /// Whether type member `s` of a class other than an object overrides a member of a base
    /// class of its class.
    fn overrides_type_member(&mut self, s: SymbolId) -> bool {
        let owner = self.u.symbol(s).owner;
        let Some(class) =
            owner.filter(|&o| self.u.symbol(o).kind == Kind::Class && !self.u.is_module_class(o))
        else {
            return false;
        };
        let name = self.u.symbol(s).name.clone();
        let bases = self.u.linearization(class);
        bases.into_iter().filter(|&b| b != class).any(|b| self.u.decl(b, &name).is_ok_and(|d| !d.is_empty()))
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
