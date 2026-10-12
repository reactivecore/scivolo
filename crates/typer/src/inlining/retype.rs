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
    /// `t`, owned by `owner`, retyped, the proxies of inline arguments in `args` replaced by copies
    /// of them and the inline calls in it expanded.
    pub fn retype(
        &mut self,
        t: TNodeId,
        args: &FxHashMap<SymbolId, TNodeId>,
        owner: SymbolId,
    ) -> Result<TNodeId, Unsupported> {
        let t = self.retype_node(t, args, owner)?;
        let Some(def) = self.inline_method(t)? else { return Ok(t) };
        self.inline_call(t, def, owner)
    }

    fn retype_node(
        &mut self,
        t: TNodeId,
        args: &FxHashMap<SymbolId, TNodeId>,
        owner: SymbolId,
    ) -> Result<TNodeId, Unsupported> {
        let span = self.tpd.span(t);
        match self.tpd.tree(t).clone() {
            T::Ident { .. } => match self.tpd.sym(t).and_then(|s| args.get(&s).map(|&r| (s, r))) {
                Some((proxy, rhs)) => {
                    let mut m = TreeTypeMap {
                        old_owner: Some(proxy),
                        new_owner: Some(owner),
                        ..TreeTypeMap::default()
                    };
                    self.copy(&mut m, rhs, &mut |_, _, _| None)
                }
                None => self.ident_as_select(t),
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
                let cond = self.retype(cond, args, owner)?;
                if let Some(C::Boolean(b)) = self.constant_of(cond) {
                    if !self.is_idempotent_expr(cond) {
                        return Err((span, "inline if on an impure constant condition"));
                    }
                    return self.retype(if b { thenp } else { elsep }, args, owner);
                }
                let (thenp, elsep) = (self.retype(thenp, args, owner)?, self.retype(elsep, args, owner)?);
                let tpe = self.u.lub(self.tpd.tpe(thenp), self.tpd.tpe(elsep));
                Ok(self.tpd.node(T::If { cond, thenp, elsep }, span, Some(tpe), None))
            }
            T::Select { qualifier, name } => {
                let before = self.tpd.get(qualifier).tpe;
                let qualifier = self.retype(qualifier, args, owner)?;
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
                self.retyped(t, owner)
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
                let (inner, call) = match *self.tpd.tree(t) {
                    T::ValDef { .. } | T::DefDef { .. } => (self.tpd.sym(t).unwrap_or(owner), None),
                    T::Inlined { call, .. } => (owner, call),
                    _ => (owner, None),
                };
                let mut failed = None;
                map_children(self, t, &mut |i, c| match Some(c) == call {
                    true => c,
                    false => i.retype(c, args, inner).unwrap_or_else(|e| {
                        failed.get_or_insert(e);
                        c
                    }),
                });
                failed.map_or(Ok(()), Err)?;
                self.retyped(t, owner)
            }
            _ => Err((span, "inline body with a match, try or return")),
        }
    }

    /// Node `t`, whose children are retyped, retyped itself.
    fn retyped(&mut self, t: TNodeId, owner: SymbolId) -> Result<TNodeId, Unsupported> {
        let applied = matches!(self.tpd.tree(t), T::Apply { .. });
        let t = self.beta_reduce(t, owner)?;
        match *self.tpd.tree(t) {
            T::Select { .. } | T::Apply { .. } => {
                self.fold_constants(t);
                Ok(self.const_to_literal(t))
            }
            _ if applied => Ok(self.const_to_literal(t)),
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

    /// Gives selection or application `t` the constant type of its operation on constants, if it
    /// is one (dotty's `ConstFold`, which also runs when trees are copied).
    pub fn fold_constants(&mut self, t: TNodeId) {
        let folded = match *self.tpd.tree(t) {
            T::Select { qualifier, name } => {
                let owner = self.tpd.sym(t).and_then(|s| self.u.symbol(s).owner).map(|o| self.u.full_name(o));
                let primitive = owner.is_some_and(|o| PRIMITIVE_OWNERS.contains(&o.as_str()));
                let x = if primitive { self.constant_operand(qualifier) } else { None };
                x.and_then(|x| fold_unop(self.names.text(name.name), &x))
            }
            T::Apply { fun, args } => match (self.tpd.tree(fun).clone(), self.tpd.items(args)) {
                (T::Select { qualifier, name }, &[arg]) => {
                    match (self.constant_operand(qualifier), self.constant_operand(arg)) {
                        (Some(x), Some(y)) => fold_binop(self.names.text(name.name), &x, &y),
                        _ => None,
                    }
                }
                _ => None,
            },
            _ => None,
        };
        if let Some(c) = folded {
            self.tpd.get_mut(t).tpe = Some(constant_type_of(self.u, &c));
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
