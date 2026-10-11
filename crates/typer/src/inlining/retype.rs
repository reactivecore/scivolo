//! The typing pass over a copied inline body (dotty's `InlineTyper`, a `ReTyper`): inline
//! arguments in place of their proxies, operations on constants folded into literals, `if`s on
//! constants reduced, and the types of `if`s, blocks and inlined arguments recomputed.

use super::copy::TreeTypeMap;
use super::purity::PRIMITIVE_OWNERS;
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T, constant_type_of, map_children};
use crate::typer::const_ops::{fold_binop, fold_unop};
use rustc_hash::FxHashMap;
use syntax::ast::constant::Constant as C;
use tasty::{SymbolId, Type, TypeId};

impl Inlining<'_> {
    /// `t` retyped, the proxies of inline arguments in `args` replaced by copies of them.
    pub fn retype(
        &mut self,
        t: TNodeId,
        args: &FxHashMap<SymbolId, TNodeId>,
    ) -> Result<TNodeId, Unsupported> {
        let span = self.tpd.span(t);
        match self.tpd.tree(t).clone() {
            T::Ident { .. } => match self.tpd.sym(t).and_then(|s| args.get(&s)) {
                Some(&rhs) => self.copy(&mut TreeTypeMap::default(), rhs, &mut |_, _, _| None),
                None => Ok(t),
            },
            T::Literal { .. } | T::ClassLiteral { .. } | T::This { .. } => Ok(t),
            T::TypeTree { .. }
            | T::SingletonTypeTree { .. }
            | T::ByNameTypeTree { .. }
            | T::AppliedTypeTree { .. }
            | T::TypeBoundsTree { .. }
            | T::LambdaTypeTree { .. }
            | T::Annotated { .. } => Ok(t),
            T::If { cond, thenp, elsep } => {
                let cond = self.retype(cond, args)?;
                if let Some(C::Boolean(b)) = self.constant_of(cond) {
                    if !self.is_idempotent_expr(cond) {
                        return Err((span, "inline if on an impure constant condition"));
                    }
                    return self.retype(if b { thenp } else { elsep }, args);
                }
                let (thenp, elsep) = (self.retype(thenp, args)?, self.retype(elsep, args)?);
                let tpe = self.u.lub(self.tpd.tpe(thenp), self.tpd.tpe(elsep));
                Ok(self.tpd.node(T::If { cond, thenp, elsep }, span, Some(tpe), None))
            }
            T::Select { qualifier, name } => {
                let before = self.tpd.get(qualifier).tpe;
                let qualifier = self.retype(qualifier, args)?;
                if let T::Select { qualifier: q, .. } = &mut self.tpd.get_mut(t).tree {
                    *q = qualifier;
                }
                if self.reselects_override(t, qualifier, self.names.text(name.name).to_owned()) {
                    return Err((span, "inline expansion selecting an overriding member"));
                }
                if let Some(b) = before
                    && self.is_stable_ref(b)
                    && let Some(tpe) = self.tpd.get(t).tpe
                    && let Type::Sym(Some(_), s) = *self.u.types().get(tpe)
                {
                    self.tpd.get_mut(t).tpe = Some(self.u.intern_type(Type::Sym(Some(b), s)));
                }
                self.retyped(t)
            }
            T::Apply { .. }
            | T::TypeApply { .. }
            | T::Typed { .. }
            | T::NamedArg { .. }
            | T::New { .. }
            | T::Block { .. }
            | T::ValDef { .. }
            | T::DefDef { .. }
            | T::Inlined { .. }
            | T::Closure { .. }
            | T::SeqLiteral { .. }
            | T::WhileDo { .. }
            | T::Assign { .. } => {
                let mut failed = None;
                map_children(self, t, &mut |i, c| match i.retype(c, args) {
                    Ok(n) => n,
                    Err(e) => {
                        failed.get_or_insert(e);
                        c
                    }
                });
                if let Some(e) = failed {
                    return Err(e);
                }
                self.retyped(t)
            }
            _ => Err((span, "inline body with a match, try or return")),
        }
    }

    /// Node `t`, whose children are retyped, retyped itself.
    fn retyped(&mut self, t: TNodeId) -> Result<TNodeId, Unsupported> {
        match *self.tpd.tree(t) {
            T::Select { qualifier, name } => {
                let owner = self.tpd.sym(t).and_then(|s| self.u.symbol(s).owner).map(|o| self.u.full_name(o));
                if owner.is_some_and(|o| PRIMITIVE_OWNERS.contains(&o.as_str()))
                    && let Some(c) =
                        self.constant_of(qualifier).and_then(|x| fold_unop(self.names.text(name.name), &x))
                {
                    self.tpd.get_mut(t).tpe = Some(constant_type_of(self.u, &c));
                }
                self.const_to_literal(t)
            }
            T::Apply { fun, args } => {
                if let T::Select { qualifier, name } = *self.tpd.tree(fun)
                    && let &[arg] = self.tpd.items(args)
                    && let (Some(x), Some(y)) = (self.constant_of(qualifier), self.constant_of(arg))
                    && let Some(c) = fold_binop(self.names.text(name.name), &x, &y)
                {
                    self.tpd.get_mut(t).tpe = Some(constant_type_of(self.u, &c));
                }
                self.const_to_literal(t)
            }
            T::Block { stats, expr } => {
                let locals: Vec<SymbolId> =
                    self.tpd.items(stats).iter().filter_map(|&s| self.tpd.sym(s)).collect();
                let tpe = self.tpd.tpe(expr);
                if !locals.iter().any(|&l| self.mentions(tpe, l)) {
                    self.tpd.get_mut(t).tpe = Some(tpe);
                }
                Ok(t)
            }
            T::Inlined { expansion, .. } => {
                self.tpd.get_mut(t).tpe = Some(self.tpd.tpe(expansion));
                Ok(t)
            }
            _ => Ok(t),
        }
    }

    /// Whether selection `t` of member `name` from `qualifier` finds, in the class of the
    /// qualifier's type, a member of that name before the owner of the selected symbol, which
    /// dotty's retyping would select instead.
    fn reselects_override(&mut self, t: TNodeId, qualifier: TNodeId, name: String) -> bool {
        let (Some(sym), Some(q)) = (self.tpd.sym(t), self.tpd.get(qualifier).tpe) else { return false };
        let owner = self.u.symbol(sym).owner;
        let widened = self.u.widen(q);
        let Some(c) = self.u.class_symbol(widened) else { return false };
        for base in self.u.linearization(c) {
            if Some(base) == owner {
                return false;
            }
            let decls = self.u.decls(base).unwrap_or_default();
            if decls.iter().any(|&d| self.u.symbol(d).name == name && self.u.symbol(d).kind.is_term()) {
                return true;
            }
        }
        false
    }

    /// Whether `t` is a reference to a stable local term or a `this`.
    fn is_stable_ref(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::This(_) => true,
            Type::Sym(None, s) => self.u.symbol(s).kind.is_term() && self.is_stable_member(s),
            _ => false,
        }
    }
}
