// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! Completing script symbols: their infos from their signatures, typed ahead where a result type
//! is inferred (dotty's `Completer`, `ClassCompleter`, `typedAhead`).

use crate::ctx::Ctx;
use crate::symbols::Completion;
use crate::typer::{InlineDef, Typer};
use names::nme;
use syntax::ast::{NodeId, Span, Tree};
use tasty::{Kind, NameId, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Completes the info of `sym` if it is a script symbol not completed yet.
    pub fn complete(&mut self, sym: SymbolId) {
        let (tree, ctx) = match self.syms.completions.get(&sym) {
            Some(&Completion::Pending { tree, ctx }) => (tree, ctx),
            Some(&Completion::Completing(tree)) if !matches!(self.ast.tree(tree), Tree::TypeDef { .. }) => {
                self.unsupported(tree, "cyclic reference");
                let error = self.u.intern_type(Type::Error);
                return self.u.set_info(sym, error);
            }
            Some(&Completion::Completing(tree)) if self.syms.checking.contains(&sym) => {
                self.unsupported(tree, "cyclic reference");
                return;
            }
            _ if self.syms.incomplete.contains(&sym) => {
                return self.unsupported_at(Span::NONE, "definition left incomplete by a failed unit");
            }
            _ => return self.complete_forwarded(sym),
        };
        self.outside_gadt_cases(ctx.level, |t| t.complete_pending(sym, tree, ctx));
    }

    fn complete_pending(&mut self, sym: SymbolId, tree: NodeId, ctx: Ctx) {
        self.syms.completions.insert(sym, Completion::Completing(tree));
        let object_val = self.is_module_val(sym);
        let import_path_objects = if object_val { None } else { self.import_path_objects.take() };
        let span = self.ast.span(tree);
        let outer_file = if span.exists() { self.enter_file(self.file_of(span)) } else { self.file };
        let reported = self.diags.items.len();
        self.set_provisional_info(sym, tree, ctx);
        let open = std::mem::take(&mut self.history.open);
        let start = self.constraint.attempt();
        let outer = self.u.set_comparison_owner(Some(ctx.owner));
        let nulls = self.null_mode(ctx);
        let outer_nulls = self.u.set_null_mode(nulls);
        self.add_annotations(sym, tree, ctx);
        let flags = self.u.symbol(sym).flags;
        if flags.has(tasty::tags::INFIX)
            && !flags.has(tasty::tags::EXTENSION)
            && !self.u.is_type_symbol(sym)
            && self.u.symbol(sym).owner.is_some_and(|o| crate::defn::is_named_package_object(self.u, o))
        {
            self.unsupported(tree, "top-level infix definition");
        }
        match self.ast.tree(tree).clone() {
            Tree::ValDef { tpt, rhs, .. } => {
                let info = match self.module_val_info(sym) {
                    Some(info) => info,
                    None => self.result_type(sym, tpt, rhs, ctx.with_owner(sym)),
                };
                self.u.set_info(sym, info);
                self.mark_erased_by_type(sym, tree, info);
            }
            Tree::DefDef { .. } => {
                if self.u.symbol(sym).flags.has(tasty::tags::INLINE) {
                    self.inline_defs.entry(sym).or_insert(InlineDef::Pending(tree, ctx));
                }
                let info = self.method_type(sym, tree, ctx);
                self.u.set_info(sym, info);
            }
            Tree::TypeDef { rhs, .. } if matches!(self.ast.tree(rhs), Tree::Template { .. }) => {
                self.complete_class(sym, (tree, rhs), ctx)
            }
            Tree::TypeDef { rhs, .. } => {
                let mctx = if self.u.symbol(sym).kind == Kind::Type { ctx.with_owner(sym) } else { ctx };
                let info = self.type_member_info(sym, rhs, mctx);
                self.u.set_info(sym, info);
                self.check_non_cyclic(sym, info, tree);
                self.check_array_self_bound(sym, info, tree);
            }
            _ => {}
        }
        self.avoid_private_leaks(sym, span);
        self.u.set_comparison_owner(outer);
        self.u.set_null_mode(outer_nulls);
        self.syms.completions.insert(sym, Completion::Done);
        if !object_val {
            self.import_path_objects = import_path_objects;
        }
        self.complete_late_forwarders(sym);
        self.record_completion_diags(reported);
        self.constraint.keep(start);
        self.history.open = open;
        self.invalidate_if_clashing(sym);
        self.enter_file(outer_file);
    }

    /// Completes the class of the top-level definitions among package statements `stats`.
    pub fn complete_top_level_class(&mut self, stats: &[NodeId]) {
        let classes = stats.iter().filter_map(|s| self.syms.by_tree.get(s).copied());
        let top =
            classes.into_iter().find(|&c| self.u.is_class(c) && self.u.symbol(c).name.ends_with("$package$"));
        if let Some(c) = top {
            self.complete(c);
        }
    }

    /// Completes the type aliases of the unit's static objects when the unit is Scala's library,
    /// whose classes read from TASTy refer to them (`scala.RuntimeException`, `Predef.String`).
    pub fn complete_library_aliases(&mut self) {
        if !self.u.compiles_scala_library() {
            return;
        }
        for s in self.pending(|u, s| u.is_static_module(s)) {
            self.complete(s);
        }
        let aliases = self.pending(|u, s| {
            u.symbol(s).kind == Kind::Type && u.symbol(s).owner.is_some_and(|o| u.is_static_module(o))
        });
        for s in aliases {
            self.complete(s);
        }
    }

    /// The symbols not completed yet that satisfy `p`, in the order they were entered.
    fn pending(&self, p: impl Fn(&tasty::Universe, SymbolId) -> bool) -> Vec<SymbolId> {
        let pending = self.syms.completions.iter().filter(|(_, c)| matches!(c, Completion::Pending { .. }));
        let mut found: Vec<SymbolId> = pending.map(|(&s, _)| s).filter(|&s| p(self.u, s)).collect();
        found.sort_unstable_by_key(|s| s.0);
        found
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
        self.add_tracked_if_needed(sym, &clauses, ctx);
        let is_value = |c: &&Vec<SymbolId>| c.first().is_some_and(|&p| !self.u.is_type_symbol(p));
        let value_clauses = clauses.iter().filter(is_value).cloned().collect();
        self.syms.param_clauses.insert(sym, value_clauses);
        let result = if self.u.symbol(sym).name == nme::CONSTRUCTOR {
            let class = self.u.symbol(sym).owner.expect("a constructor's class");
            let class_ref = crate::defn::type_ref(self.u, class);
            let tparams: Vec<SymbolId> =
                clauses.iter().flatten().copied().filter(|&p| self.u.is_type_symbol(p)).collect();
            if tparams.is_empty() {
                class_ref
            } else {
                let args: Vec<_> = tparams.iter().map(|&p| self.u.intern_type(Type::Sym(None, p))).collect();
                self.u.intern_applied(class_ref, &args)
            }
        } else {
            if !self.is_inferred_tpt(tpt) {
                self.syms.declared_results.insert(sym);
            }
            self.result_type(sym, tpt, rhs, mctx)
        };
        self.syms.def_results.insert(sym, result);
        let result = self.given_result_refined(sym, &clauses, result, ctx);
        let clauses = if self.u.symbol(sym).name == nme::CONSTRUCTOR {
            self.normalized_constructor(clauses)
        } else {
            clauses
        };
        self.clauses_info(clauses, result)
    }

    /// The info of a method with parameter clauses `clauses` and result type `result`, both
    /// referring to the parameters by their symbols.
    pub fn clauses_info(&mut self, clauses: Vec<Vec<SymbolId>>, result: TypeId) -> TypeId {
        clauses.into_iter().rev().fold(result, |result, clause| {
            let binder = self.syms.fresh_binder();
            let params: Vec<(NameId, TypeId)> =
                clause.iter().map(|&p| (self.u.interned_name(p), self.method_param_type(p))).collect();
            if clause.first().is_some_and(|&p| self.u.is_type_symbol(p)) {
                let params: Vec<_> =
                    params.into_iter().map(|(n, i)| (n, self.param_refs(i, &clause, binder))).collect();
                let result = self.param_refs(result, &clause, binder);
                self.u.intern_poly(binder, &params, result)
            } else {
                let flags = clause.first().and_then(|p| self.syms.flags.get(p)).copied().unwrap_or_default();
                let contextual = flags.is(syntax::ast::flags::GIVEN);
                let implicit = contextual || flags.is(syntax::ast::flags::IMPLICIT);
                self.u.intern_method(binder, &params, result, implicit, contextual)
            }
        })
    }
}
