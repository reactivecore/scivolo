//! Completing script symbols: their infos from their signatures, typed ahead where a result type
//! is inferred (dotty's `Completer`, `ClassCompleter`, `typedAhead`).

use crate::ctx::{Ctx, Level};
use crate::symbols::Completion;
use crate::typer::{Pt, Typer};
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

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
            _ => return,
        };
        self.syms.completions.insert(sym, Completion::Completing(tree));
        match self.ast.tree(tree).clone() {
            Tree::ValDef { tpt, rhs, .. } => {
                let info = self.result_type(sym, tpt, rhs, ctx.with_owner(sym));
                self.u.set_info(sym, info);
            }
            Tree::DefDef { .. } => {
                let info = self.method_type(sym, tree, ctx);
                self.u.set_info(sym, info);
            }
            Tree::TypeDef { rhs, .. } => self.complete_class(sym, rhs, ctx),
            _ => {}
        }
        self.syms.completions.insert(sym, Completion::Done);
    }

    /// The type of definition `sym`: its type tree's, else its right-hand side's, widened.
    fn result_type(&mut self, sym: SymbolId, tpt: NodeId, rhs: Option<NodeId>, ctx: Ctx) -> TypeId {
        if !matches!(self.ast.tree(tpt), Tree::TypeTree) {
            let typed = self.typed_type(tpt, ctx);
            self.ahead.insert(tpt, typed);
            return self.tpd.tpe(typed);
        }
        let Some(rhs) = rhs else { return self.u.intern_type(Type::Error) };
        let typed = self.typed(rhs, Pt::Any, ctx);
        self.ahead.insert(rhs, typed);
        let tpe = self.tpd.tpe(typed);
        self.widen_rhs(sym, tpe)
    }

    /// The context of the parameters and body of method `sym`, defined in `ctx`.
    pub fn method_ctx(&mut self, sym: SymbolId, ctx: Ctx) -> Ctx {
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        Ctx { owner: sym, level, ..ctx }
    }

    /// The parameter symbols of method definition `tree`, entered in `ctx`, by clause.
    pub fn enter_params(&mut self, sym: SymbolId, tree: NodeId, ctx: Ctx) -> Vec<Vec<SymbolId>> {
        let mut out = Vec::new();
        for clause in self.def_clauses(tree) {
            let mut syms = Vec::new();
            for p in clause {
                let param = match self.syms.by_tree.get(&p) {
                    Some(&s) => s,
                    None => self.define_param(sym, p, ctx),
                };
                let name = self.u.symbol(param).name.clone();
                self.levels.enter(ctx.level, &name, param);
                syms.push(param);
            }
            out.push(syms);
        }
        out
    }

    fn define_param(&mut self, method: SymbolId, p: NodeId, ctx: Ctx) -> SymbolId {
        let Tree::ValDef { name, mods, tpt, .. } = *self.ast.tree(p) else {
            let s = self.syms.define(self.u, "?", Kind::Param, method, syntax::ast::flags::EMPTY);
            self.unsupported(p, "type parameter");
            return s;
        };
        let flags = self.ast.modifiers(mods).flags.or(syntax::ast::flags::TERM_PARAM).to_term_flags();
        let text = self.text(name.name).to_owned();
        let s = self.syms.define(self.u, &text, Kind::Param, method, flags);
        self.syms.by_tree.insert(p, s);
        let typed = self.typed_type(tpt, ctx);
        self.ahead.insert(tpt, typed);
        let tpe = self.tpd.tpe(typed);
        self.u.set_info(s, tpe);
        s
    }

    fn method_type(&mut self, sym: SymbolId, tree: NodeId, ctx: Ctx) -> TypeId {
        let Tree::DefDef { tpt, rhs, .. } = *self.ast.tree(tree) else { unreachable!() };
        let mctx = self.method_ctx(sym, ctx);
        let clauses = self.enter_params(sym, tree, mctx);
        let result = if self.u.symbol(sym).name == "<init>" {
            let class = self.u.symbol(sym).owner.expect("a constructor's class");
            crate::defn::type_ref(self.u, class)
        } else {
            self.result_type(sym, tpt, rhs, mctx)
        };
        clauses.into_iter().rev().fold(result, |result, clause| {
            let params = clause.iter().map(|&p| (self.u.symbol(p).name.clone(), self.u.info(p))).collect();
            let binder = self.syms.fresh_binder();
            self.u.intern_type(Type::Method { binder, params, result, implicit: false })
        })
    }

    /// The type inferred for definition `sym` from its right-hand side's type `t` (dotty's
    /// `widenRhs`): a constant for a final value, a singleton for an object, else `t` widened.
    fn widen_rhs(&mut self, sym: SymbolId, t: TypeId) -> TypeId {
        let flags = self.syms.flags.get(&sym).copied().unwrap_or_default();
        let inline_val = flags.is(f::FINAL.or(f::INLINE)) && !flags.is(f::MUTABLE) && !flags.is(f::METHOD);
        match self.u.types().get(t).clone() {
            Type::Constant(..) | Type::Str(_) if inline_val => t,
            Type::Constant(..) | Type::Str(_) => self.u.widen(t),
            Type::Sym(_, s) if self.u.symbol(s).flags.has(tasty::tags::OBJECT) => t,
            Type::Sym(_, s) if !self.is_type_symbol(s) && self.u.symbol(s).kind != Kind::Package => {
                self.complete(s);
                let info = self.u.info(s);
                self.widen_rhs(sym, info)
            }
            _ => t,
        }
    }
}
