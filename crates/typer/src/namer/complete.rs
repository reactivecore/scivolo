//! Completing script symbols: their infos from their signatures, typed ahead where a result type
//! is inferred (dotty's `Completer`, `ClassCompleter`, `typedAhead`).

use crate::ctx::Ctx;
use crate::symbols::Completion;
use crate::typer::deskolemize::deskolemized;
use crate::typer::{InlineDef, Pt, Typer, Widen};
use syntax::ast::{NodeId, Span, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Completes the info of `sym` if it is a script symbol not completed yet.
    pub fn complete(&mut self, sym: SymbolId) {
        let (tree, ctx) = match self.syms.completions.get(&sym) {
            Some(&Completion::Pending { tree, ctx }) => (tree, ctx),
            Some(&Completion::Completing(tree)) if !self.is_class_def(tree) => {
                self.unsupported(tree, "cyclic reference");
                let error = self.u.intern_type(Type::Error);
                return self.u.set_info(sym, error);
            }
            _ if self.syms.incomplete.contains(&sym) => {
                return self.unsupported_at(Span::NONE, "definition left incomplete by a failed unit");
            }
            _ => return,
        };
        self.syms.completions.insert(sym, Completion::Completing(tree));
        let open = std::mem::take(&mut self.history.open);
        let vars = self.constraint.count();
        self.add_annotations(sym, tree, ctx);
        match self.ast.tree(tree).clone() {
            Tree::ValDef { tpt, rhs, .. } => {
                let info = self.result_type(sym, tpt, rhs, ctx.with_owner(sym));
                self.u.set_info(sym, info);
            }
            Tree::DefDef { .. } => {
                if self.u.symbol(sym).flags.has(tasty::tags::INLINE) {
                    self.inline_defs.entry(sym).or_insert(InlineDef::Pending(tree, ctx));
                }
                let info = self.method_type(sym, tree, ctx);
                self.u.set_info(sym, info);
            }
            Tree::TypeDef { rhs, .. } if matches!(self.ast.tree(rhs), Tree::Template { .. }) => {
                self.complete_class(sym, rhs, ctx)
            }
            Tree::TypeDef { rhs, .. } => {
                let info = self.type_def_info(rhs, ctx);
                self.u.set_info(sym, info);
                self.check_non_cyclic(sym, info, tree);
            }
            _ => {}
        }
        self.syms.completions.insert(sym, Completion::Done);
        self.constraint.keep_from(vars);
        self.history.open = open;
    }

    fn is_class_def(&self, tree: NodeId) -> bool {
        matches!(*self.ast.tree(tree), Tree::TypeDef { rhs, .. } if matches!(self.ast.tree(rhs), Tree::Template { .. }))
    }

    /// The type of definition `sym`: its type tree's, else its right-hand side's, widened.
    fn result_type(&mut self, sym: SymbolId, tpt: NodeId, rhs: Option<NodeId>, ctx: Ctx) -> TypeId {
        if !self.is_inferred_tpt(tpt) {
            return self.value_type_ahead(tpt, ctx);
        }
        let Some(rhs) = rhs else { return self.u.intern_type(Type::Error) };
        let inherited = self.inherited_result(sym);
        if inherited.is_none() {
            self.check_inferred_implicit(sym);
        }
        let (raw_default, by_name) = self.default_param_proto(sym);
        let default = raw_default.filter(|&d| !self.mentions_own_type_params(d, sym));
        self.check_context_default(rhs, raw_default.filter(|_| default.is_none()));
        let lambda = matches!(self.ast.tree(rhs), Tree::Function { .. });
        let approx = raw_default.filter(|_| lambda && default.is_none()).and_then(|d| {
            let a = self.own_type_params_approximated(d, sym);
            if a.is_none() {
                self.unsupported(rhs, "function default of a bounded type parameter");
            }
            a
        });
        let pt = inherited.or(default).or(approx).map_or(Pt::Any, Pt::Type);
        let typed = self.typed(rhs, pt, ctx);
        self.typed_ahead(rhs, typed, ctx);
        let tpe = self.tpd.tpe(typed);
        let tpe = self.fully_defined_as(tpe, true).unwrap_or_else(|what| {
            self.unsupported(rhs, what);
            tpe
        });
        match (inherited, default.filter(|_| !by_name)) {
            (Some(i), _) if !self.keeps_rhs_type(sym) => i,
            (None, Some(d)) if self.u.is_subtype(tpe, d) => self.default_getter_type(d),
            _ => {
                let widened = self.widen_rhs(sym, tpe);
                let widened = self.simplified(widened);
                if self.keeps_rhs_type(sym)
                    && matches!(self.u.types().get(widened), Type::Constant(..) | Type::Str(_))
                {
                    return widened;
                }
                let bound = inherited.or(default).or(approx).unwrap_or(tasty::ANY);
                let widened = self.widen_inferred(widened, bound, Widen::Unions).unwrap_or_else(|what| {
                    self.unsupported(rhs, what);
                    widened
                });
                deskolemized(self.u, widened, 1).unwrap_or_else(|| {
                    self.unsupported(rhs, "skolem in an inferred type");
                    widened
                })
            }
        }
    }

    /// The type of value type tree `tpt`, typed ahead in `ctx`.
    pub fn value_type_ahead(&mut self, tpt: NodeId, ctx: Ctx) -> TypeId {
        let typed = self.typed_type(tpt, ctx);
        self.typed_ahead(tpt, typed, ctx);
        let tpe = self.tpd.tpe(typed);
        self.check_simple_kinded(tpe, tpt);
        tpe
    }

    fn method_type(&mut self, sym: SymbolId, tree: NodeId, ctx: Ctx) -> TypeId {
        let Tree::DefDef { tpt, rhs, .. } = *self.ast.tree(tree) else { unreachable!() };
        let mctx = self.method_ctx(sym, ctx);
        let clauses = self.enter_params(sym, tree, mctx);
        let is_value = |c: &&Vec<SymbolId>| c.first().is_some_and(|&p| !self.u.is_type_symbol(p));
        let value_clauses = clauses.iter().filter(is_value).cloned().collect();
        self.syms.param_clauses.insert(sym, value_clauses);
        let result = if self.u.symbol(sym).name == "<init>" {
            let class = self.u.symbol(sym).owner.expect("a constructor's class");
            let class_ref = crate::defn::type_ref(self.u, class);
            let tparams: Vec<SymbolId> =
                clauses.iter().flatten().copied().filter(|&p| self.u.is_type_symbol(p)).collect();
            if tparams.is_empty() {
                class_ref
            } else {
                let args = tparams.iter().map(|&p| self.u.intern_type(Type::Sym(None, p))).collect();
                self.u.intern_type(Type::Applied(class_ref, args))
            }
        } else {
            if !self.is_inferred_tpt(tpt) {
                self.syms.declared_results.insert(sym);
            }
            self.result_type(sym, tpt, rhs, mctx)
        };
        self.syms.def_results.insert(sym, result);
        let clauses =
            if self.u.symbol(sym).name == "<init>" { self.normalized_constructor(clauses) } else { clauses };
        clauses.into_iter().rev().fold(result, |result, clause| {
            let binder = self.syms.fresh_binder();
            let params: Vec<(String, TypeId)> =
                clause.iter().map(|&p| (self.u.symbol(p).name.clone(), self.method_param_type(p))).collect();
            if clause.first().is_some_and(|&p| self.u.is_type_symbol(p)) {
                let params =
                    params.into_iter().map(|(n, i)| (n, self.param_refs(i, &clause, binder))).collect();
                let result = self.param_refs(result, &clause, binder);
                self.u.intern_type(Type::Poly { binder, params, result })
            } else {
                let flags = clause.first().and_then(|p| self.syms.flags.get(p)).copied().unwrap_or_default();
                let contextual = flags.is(syntax::ast::flags::GIVEN);
                let implicit = contextual || flags.is(syntax::ast::flags::IMPLICIT);
                self.u.intern_type(Type::Method { binder, params, result, implicit, contextual })
            }
        })
    }

    /// `t` with references to type parameters `tparams` made references to the parameters of
    /// polymorphic type `binder`.
    fn param_refs(&mut self, t: TypeId, tparams: &[SymbolId], binder: tasty::Binder) -> TypeId {
        self.u.map_type(t, &mut |u, x| match *u.types().get(x) {
            Type::Sym(None, s) => {
                let i = tparams.iter().position(|&p| p == s)?;
                Some(u.intern_type(Type::Param(binder, i as u32)))
            }
            _ => None,
        })
    }

    /// The type inferred for definition `sym` from its right-hand side's type `t` (dotty's
    /// `widenRhs`): a constant for a final value, a singleton for an object, else `t` widened.
    fn widen_rhs(&mut self, sym: SymbolId, t: TypeId) -> TypeId {
        let inline_val = self.keeps_rhs_type(sym);
        match self.u.types().get(t).clone() {
            Type::Constant(..) | Type::Str(_) if inline_val => t,
            Type::Constant(..) | Type::Str(_) => self.u.widen(t),
            Type::Annotated(seq, annot) if self.is_repeated_annot(annot) => seq,
            Type::Sym(_, s) if self.u.symbol(s).flags.has(tasty::tags::OBJECT) => t,
            Type::Sym(_, s) if !self.u.is_type_symbol(s) && self.u.symbol(s).kind != Kind::Package => {
                let info = self.method_info(t).unwrap_or(t);
                self.widen_rhs(sym, info)
            }
            Type::ByName(r) | Type::Skolem(_, r) => self.widen_rhs(sym, r),
            Type::This(c) => self.widened_this(t, c),
            _ => t,
        }
    }
}
