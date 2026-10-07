//! Parts of applications lifted into local values, evaluated once and in order (dotty's
//! `Lifter`, `LiftComplex` if `complex`, else `LiftImpure`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{Span, TName};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// Whether `expr` stays in place (dotty's `noLift`).
    fn no_lift(&mut self, expr: TNodeId, complex: bool) -> bool {
        if complex { self.is_pure_path(expr) } else { self.is_pure_expr(expr) }
    }

    /// Whether `t` is a stable type (dotty's `isStable`): a reference to a stable member through a
    /// stable prefix, a `this`, a package, a constant.
    fn is_stable_type(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::This(_) | Type::Package(_) | Type::Constant(..) | Type::Str(_) => true,
            Type::Sym(prefix, s) => {
                let kind = self.u.symbol(s).kind;
                kind == Kind::Package
                    || !self.is_type_symbol(s)
                        && self.is_stable_member(s)
                        && prefix.is_none_or(|p| self.is_stable_type(p))
            }
            _ => false,
        }
    }

    /// `expr`, or a reference to `val prefix$N = expr` added to `defs` if it needs lifting
    /// (dotty's `lift`); `$N$` for an empty prefix.
    pub fn lift(
        &mut self,
        defs: &mut Vec<TNodeId>,
        expr: TNodeId,
        prefix: &str,
        complex: bool,
        ctx: Ctx,
    ) -> TNodeId {
        if self.no_lift(expr, complex) {
            return expr;
        }
        let name = match prefix {
            "" => self.fresh.unique(self.names),
            p => self.fresh.next(self.names, &format!("{p}$")),
        };
        let text = self.text(name).to_owned();
        let tpe = self.tpd.tpe(expr);
        let tpe = if self.is_stable_type(tpe) { tpe } else { self.u.widen(tpe) };
        let tpe = self.fully_defined(tpe).unwrap_or(tpe);
        let sym = self.syms.define(self.u, &text, Kind::Val, ctx.owner, f::SYNTHETIC);
        self.u.set_info(sym, tpe);
        self.syms.closure_syms.push(sym);
        self.change_owners(expr, ctx.owner, sym);
        let span = self.tpd.span(expr);
        let tpt = self.type_tree(tpe, span.start_pos(), true);
        let name = TName { name, is_type: false };
        defs.push(self.tpd.node(T::ValDef { name, tpt, rhs: Some(expr) }, span, None, Some(sym)));
        let reference = self.u.intern_type(Type::Sym(None, sym));
        self.tpd.node(T::Ident { name }, Span::at(span.point()), Some(reference), Some(sym))
    }

    /// Makes `to` the owner of the definitions in `tree` that `from` owns (dotty's
    /// `changeNonLocalOwners`).
    fn change_owners(&mut self, tree: TNodeId, from: tasty::SymbolId, to: tasty::SymbolId) {
        let mut todo = vec![tree];
        while let Some(t) = todo.pop() {
            let definition = matches!(
                self.tpd.tree(t),
                T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Bind { .. }
            );
            if definition
                && let Some(s) = self.tpd.sym(t)
                && self.u.symbol(s).owner == Some(from)
            {
                self.u.set_owner(s, to);
            }
            todo.extend(self.tpd.children(t));
        }
    }

    /// Call `f` with its prefix lifted unless idempotent, or for a method with default
    /// arguments unless a literal, `this`, `new` or an idempotent reference (dotty's
    /// `liftApp`, `liftPrefix`); `Err` for a function that is a call or a block.
    pub fn lift_app(
        &mut self,
        defs: &mut Vec<TNodeId>,
        f: TNodeId,
        complex: bool,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let span = self.tpd.span(f);
        match *self.tpd.tree(f) {
            T::TypeApply { fun, args } => {
                let lifted = self.lift_app(defs, fun, complex, ctx)?;
                if lifted == fun {
                    return Ok(f);
                }
                let tpe = self.tpd.get(f).tpe;
                Ok(self.tpd.node(T::TypeApply { fun: lifted, args }, span, tpe, None))
            }
            T::Select { qualifier, name } => {
                let sym = self.tpd.sym(f).ok_or("lifting")?;
                let defaults = self.has_defaults(sym);
                let keep = match *self.tpd.tree(qualifier) {
                    T::Literal { .. } | T::This { .. } | T::New { .. } => true,
                    T::Ident { .. } | T::Select { .. } => self.is_idempotent_expr(qualifier),
                    _ => !defaults && self.is_idempotent_expr(qualifier),
                };
                let lifted = if keep { qualifier } else { self.lift(defs, qualifier, "", complex, ctx) };
                if lifted == qualifier {
                    return Ok(f);
                }
                let site = self.tpd.tpe(lifted);
                let tpe = self.ref_type(Some(site), sym);
                Ok(self.tpd.node(T::Select { qualifier: lifted, name }, span, Some(tpe), Some(sym)))
            }
            T::New { .. } => Ok(f),
            T::Ident { .. } => Ok(self.lift(defs, f, "", complex, ctx)),
            _ => Err("lifting of a call or block"),
        }
    }

    /// `args`, of parameters `params`, each lifted unless it needs not or its parameter is by-name,
    /// named after its parameter unless the name has a `$` (dotty's `liftArgs`); the definitions
    /// added to `defs` by argument index.
    pub fn lift_args(
        &mut self,
        defs: &mut Vec<(usize, TNodeId)>,
        params: &[(String, TypeId)],
        args: &[TNodeId],
        complex: bool,
        ctx: Ctx,
    ) -> Vec<TNodeId> {
        let mut out = Vec::new();
        for (i, (&arg, (name, p))) in args.iter().zip(params).enumerate() {
            if matches!(self.u.types().get(*p), Type::ByName(_)) {
                out.push(arg);
                continue;
            }
            let prefix = if name.contains('$') { String::new() } else { name.clone() };
            let mut own = Vec::new();
            let lifted = match *self.tpd.tree(arg) {
                T::NamedArg { name, arg: inner } => {
                    let lifted = self.lift(&mut own, inner, &prefix, complex, ctx);
                    let (span, tpe) = (self.tpd.span(arg), self.tpd.get(arg).tpe);
                    if lifted == inner {
                        arg
                    } else {
                        self.tpd.node(T::NamedArg { name, arg: lifted }, span, tpe, None)
                    }
                }
                _ => self.lift(&mut own, arg, &prefix, complex, ctx),
            };
            defs.extend(own.into_iter().map(|d| (i, d)));
            out.push(lifted);
        }
        out
    }

    /// The result type of method reference `f`.
    pub fn result_of(&mut self, f: TNodeId) -> Option<TypeId> {
        let info = self.method_info(self.tpd.tpe(f))?;
        match *self.u.types().get(info) {
            Type::Method { result, .. } => Some(result),
            _ => None,
        }
    }

    /// `{ defs; tree }`, or `tree` if there are no definitions (dotty's `wrapDefs`), its type
    /// widened if it refers to the definitions; outside the subset if it still does.
    pub fn wrap_defs(&mut self, defs: Vec<TNodeId>, tree: TNodeId) -> TNodeId {
        if defs.is_empty() {
            return tree;
        }
        let span = defs.iter().fold(self.tpd.span(tree), |s, &d| s.union(self.tpd.span(d)));
        let span = Span::synthetic(span.start(), span.end());
        let syms: Vec<tasty::SymbolId> = defs.iter().filter_map(|&d| self.tpd.sym(d)).collect();
        let tpe = self.tpd.get(tree).tpe.map(|t| if self.refers_to(t, &syms) { self.u.widen(t) } else { t });
        if tpe.is_some_and(|t| self.refers_to(t, &syms)) {
            self.unsupported_at(span, "type of a lifted call referring to a lifted value");
        }
        let stats = self.tpd.list(&defs);
        self.tpd.node(T::Block { stats, expr: tree }, span, tpe, None)
    }
}
