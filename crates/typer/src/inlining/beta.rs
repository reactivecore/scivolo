//! Applications of closures reduced to the closures' bodies, the arguments bound to the
//! parameters (dotty's `BetaReduce`, without polymorphic functions and anonymous classes).

use super::copy::TreeTypeMap;
use super::purity::{PURE, at_least};
use super::{Inlining, Unsupported};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use syntax::ast::constant::Constant as C;
use tasty::tags::{MUTABLE, SYNTHETIC};
use tasty::{Flags, Kind, Symbol, SymbolId, Type};

impl Inlining<'_> {
    /// `t`, owned by `owner`, beta-reduced if it applies a closure's `apply` to arguments.
    pub fn beta_reduce(&mut self, t: TNodeId, owner: SymbolId) -> Result<TNodeId, Unsupported> {
        let T::Apply { fun, args } = *self.tpd.tree(t) else { return Ok(t) };
        let T::Select { qualifier, name } = *self.tpd.tree(fun) else { return Ok(t) };
        if self.names.text(name.name) != "apply" || !self.is_function_type(qualifier) {
            return Ok(t);
        }
        let args = self.tpd.items(args).to_vec();
        let mut bindings = Vec::new();
        let Some(reduced) = self.reduce(qualifier, &args, &mut bindings, owner)? else { return Ok(t) };
        let span = self.tpd.span(t);
        if bindings.is_empty() {
            self.tpd.get_mut(reduced).span = span;
            return Ok(reduced);
        }
        let stats = self.tpd.list(&bindings);
        let tpe = self.tpd.get(reduced).tpe;
        Ok(self.tpd.node(T::Block { stats, expr: reduced }, span, tpe, None))
    }

    fn is_function_type(&mut self, t: TNodeId) -> bool {
        let Some(tpe) = self.tpd.get(t).tpe else { return false };
        let widened = self.u.widen(tpe);
        let Some(c) = self.u.class_symbol(widened) else { return false };
        let full = self.u.full_name(c);
        full.strip_prefix("scala.Function").is_some_and(|n| n.parse::<u32>().is_ok())
    }

    /// The body of closure `f` applied to `args`, through pure blocks, inlined trees and type
    /// ascriptions; its argument bindings added to `bindings`.
    fn reduce(
        &mut self,
        f: TNodeId,
        args: &[TNodeId],
        bindings: &mut Vec<TNodeId>,
        owner: SymbolId,
    ) -> Result<Option<TNodeId>, Unsupported> {
        let (span, tpe) = (self.tpd.span(f), self.tpd.get(f).tpe);
        match self.tpd.tree(f).clone() {
            T::Block { stats, expr } => {
                let stats = self.tpd.items(stats).to_vec();
                if let ([d], T::Closure { meth, .. }) = (stats.as_slice(), self.tpd.tree(expr))
                    && let T::DefDef { .. } = self.tpd.tree(*d)
                    && self.tpd.sym(*d).is_some()
                    && self.tpd.sym(*d) == self.tpd.sym(*meth)
                {
                    return self.reduce_application(*d, args, bindings, owner);
                }
                if !stats.iter().all(|&s| self.is_pure_binding(s)) {
                    return Ok(None);
                }
                let Some(e) = self.reduce(expr, args, bindings, owner)? else { return Ok(None) };
                let stats = self.tpd.list(&stats);
                Ok(Some(self.tpd.node(T::Block { stats, expr: e }, span, self.tpd.get(e).tpe.or(tpe), None)))
            }
            T::Inlined { call, bindings: defs, expansion } => {
                if !self.tpd.items(defs).to_vec().into_iter().all(|s| self.is_pure_binding(s)) {
                    return Ok(None);
                }
                let Some(e) = self.reduce(expansion, args, bindings, owner)? else { return Ok(None) };
                let tpe = self.tpd.get(e).tpe;
                Ok(Some(self.tpd.node(T::Inlined { call, bindings: defs, expansion: e }, span, tpe, None)))
            }
            T::Typed { expr, .. } => self.reduce(expr, args, bindings, owner),
            _ => Ok(None),
        }
    }

    fn is_pure_binding(&mut self, s: TNodeId) -> bool {
        match *self.tpd.tree(s) {
            T::ValDef { rhs: Some(r), .. } => {
                let mutable = self.tpd.sym(s).is_some_and(|v| self.u.symbol(v).flags.has(MUTABLE));
                !mutable && self.is_pure_expr(r)
            }
            T::DefDef { .. } => true,
            _ => false,
        }
    }

    /// The body of method `def` with its parameters bound to `args` (dotty's
    /// `reduceApplication`); `None` if the arguments do not fit the one parameter clause.
    fn reduce_application(
        &mut self,
        def: TNodeId,
        args: &[TNodeId],
        bindings: &mut Vec<TNodeId>,
        owner: SymbolId,
    ) -> Result<Option<TNodeId>, Unsupported> {
        let T::DefDef { paramss, rhs: Some(rhs), .. } = *self.tpd.tree(def) else { return Ok(None) };
        let clauses = self.tpd.clause_items(paramss).to_vec();
        let [clause] = clauses[..] else { return Ok(None) };
        let params = self.tpd.items(clause).to_vec();
        if params.len() != args.len() {
            return Ok(None);
        }
        let mut m =
            TreeTypeMap { old_owner: self.tpd.sym(def), new_owner: Some(owner), ..TreeTypeMap::default() };
        for (&p, &a) in params.iter().zip(args) {
            let Some(ps) = self.tpd.sym(p) else { return Ok(None) };
            let arg_tpe = self.tpd.tpe(a);
            let direct = match *self.u.types().get(arg_tpe) {
                Type::Sym(None, s) => {
                    self.u.symbol(s).kind.is_term() && self.purity(a) == super::purity::PURE_PATH
                }
                _ => false,
            };
            let to = if direct {
                let Type::Sym(None, s) = *self.u.types().get(arg_tpe) else { return Ok(None) };
                s
            } else {
                self.arg_binding(ps, a, bindings, owner)?
            };
            m.syms.insert(ps, to);
        }
        let body = self.copy(&mut m, rhs, &mut |_, _, _| None)?;
        Ok(Some(self.literals_for_pure_constants(body)))
    }

    /// The symbol of a new binding of argument `a` of closure parameter `p`, its definition added
    /// to `bindings` unless the argument is a pure constant or unit.
    fn arg_binding(
        &mut self,
        p: SymbolId,
        a: TNodeId,
        bindings: &mut Vec<TNodeId>,
        owner: SymbolId,
    ) -> Result<SymbolId, Unsupported> {
        let p_info = self.u.info(p);
        if matches!(self.u.types().get(p_info), Type::ByName(_)) {
            return Err((self.tpd.span(a), "beta reduction with a by-name parameter"));
        }
        let arg_tpe = self.tpd.tpe(a);
        let builtins = self.u.builtins();
        let widened = self.u.widen(arg_tpe);
        let bottom =
            self.u.class_symbol(widened).is_some_and(|c| c == builtins.nothing || c == builtins.null);
        let constant = matches!(self.u.types().get(arg_tpe), Type::Constant(..) | Type::Str(_));
        let tpe = if bottom {
            self.u.widen(p_info)
        } else if constant {
            arg_tpe
        } else {
            widened
        };
        let name = self.u.symbol(p).name.clone();
        let s = self.u.define(Symbol {
            name: name.clone(),
            kind: Kind::Val,
            owner: Some(owner),
            flags: Flags::default().with(SYNTHETIC),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.u.set_info(s, tpe);
        let unit = self.u.class_symbol(tpe).is_some_and(|c| self.u.full_name(c) == "scala.Unit");
        if !((constant || unit) && at_least(self.purity(a), PURE)) {
            let span = self.tpd.span(a);
            let tpt = self.tpd.node(
                T::TypeTree { inferred: false },
                syntax::ast::Span::at(span.start()),
                Some(tpe),
                None,
            );
            let name = TName { name: self.names.intern(&name), is_type: false };
            bindings.push(self.tpd.node(T::ValDef { name, tpt, rhs: Some(a) }, span, None, Some(s)));
        }
        Ok(s)
    }

    /// `t` with each pure subtree of a constant or unit type replaced by its literal.
    fn literals_for_pure_constants(&mut self, t: TNodeId) -> TNodeId {
        let is_term = !matches!(self.tpd.tree(t), T::TypeTree { .. } | T::AppliedTypeTree { .. });
        if is_term && let Some(tpe) = self.tpd.get(t).tpe {
            let value = self.constant_of_type(tpe).or_else(|| {
                let unit = self.u.class_symbol(tpe).is_some_and(|c| self.u.full_name(c) == "scala.Unit");
                unit.then_some(C::Unit)
            });
            if let Some(value) = value
                && !matches!(self.tpd.tree(t), T::Literal { .. })
                && self.is_pure_expr(t)
            {
                let tpe = crate::tpd::constant_type_of(self.u, &value);
                return self.tpd.node(T::Literal { value }, self.tpd.span(t), Some(tpe), None);
            }
        }
        let expr = self.result_expr(t);
        crate::tpd::map_children(self, t, &mut |i, c| i.literals_for_pure_constants(c));
        if let Some(e) = self.result_expr(t).filter(|&e| Some(e) != expr) {
            self.tpd.get_mut(t).tpe = self.tpd.get(e).tpe;
        }
        t
    }

    /// The expression giving the value of block or inlined tree `t`.
    fn result_expr(&self, t: TNodeId) -> Option<TNodeId> {
        match *self.tpd.tree(t) {
            T::Block { expr, .. } | T::Inlined { expansion: expr, .. } => Some(expr),
            _ => None,
        }
    }
}
