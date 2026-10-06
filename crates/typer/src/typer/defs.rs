//! Value and method definitions (dotty's `typedValDef`, `typedDefDef`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::{SymbolId, Type};

impl Typer<'_> {
    pub fn typed_val_def(&mut self, id: NodeId, ctx: Ctx) -> TNodeId {
        let Tree::ValDef { name, tpt, rhs, .. } = *self.ast.tree(id) else { unreachable!() };
        let sym = self.syms.by_tree.get(&id).copied();
        let info = sym.map(|s| {
            self.complete(s);
            self.u.info(s)
        });
        let rhs_ctx = sym.map_or(ctx, |s| ctx.with_owner(s));
        let tpt = match self.ready(tpt, rhs_ctx) {
            Some(typed) => typed,
            None => self.typed_tpt(tpt, info, ctx),
        };
        let pt = info.map_or(Pt::Any, Pt::Type);
        let rhs = rhs.map(|r| self.typed(r, pt, rhs_ctx));
        let span = self.ast.span(id);
        self.tpd.node(T::ValDef { name, tpt, rhs }, span, None, sym)
    }

    /// Type tree `tpt`, or, if the source gives none, an inferred one of type `info`.
    fn typed_tpt(&mut self, tpt: NodeId, info: Option<tasty::TypeId>, ctx: Ctx) -> TNodeId {
        if matches!(self.ast.tree(tpt), Tree::TypeTree) && !self.splices.contains_key(&tpt) {
            let tpe = info.unwrap_or_else(|| self.u.intern_type(Type::Error));
            let span = self.ast.span(tpt);
            return self.type_tree(tpe, span, true);
        }
        self.typed_type(tpt, ctx)
    }

    pub fn typed_def_def(&mut self, id: NodeId, ctx: Ctx) -> TNodeId {
        let Tree::DefDef { name, tpt, rhs, .. } = *self.ast.tree(id) else { unreachable!() };
        let sym = self.syms.by_tree[&id];
        self.complete(sym);
        let mctx = self.method_ctx(sym, ctx);
        self.enter_params(sym, id, mctx);
        let mut typed_clauses = Vec::new();
        for clause in self.def_clauses(id) {
            let params: Vec<TNodeId> = clause
                .iter()
                .map(|&p| match self.ast.tree(p) {
                    Tree::TypeDef { .. } => self.typed_type_def(p, mctx),
                    _ => self.typed_val_def(p, mctx),
                })
                .collect();
            typed_clauses.push(self.tpd.list(&params));
        }
        let paramss = self.tpd.clauses(&typed_clauses);
        let result = self.def_result(sym);
        let tpt = self.typed_tpt(tpt, Some(result), mctx);
        let rhs = rhs.map(|r| self.typed(r, Pt::Type(result), mctx));
        let span = self.ast.span(id);
        self.tpd.node(T::DefDef { name, paramss, tpt, rhs }, span, None, Some(sym))
    }

    /// The parameter clauses of method definition `id`.
    pub fn def_clauses(&self, id: NodeId) -> Vec<Vec<NodeId>> {
        let Tree::DefDef { paramss, .. } = *self.ast.tree(id) else { return Vec::new() };
        self.ast.clause_items(paramss).iter().map(|&l| self.ast.items(l).to_vec()).collect()
    }

    /// The result type of method `sym`'s definition; `Unit` for a constructor.
    fn def_result(&mut self, sym: SymbolId) -> tasty::TypeId {
        if self.u.symbol(sym).name == "<init>" {
            return crate::defn::type_ref(self.u, self.defn.unit);
        }
        match self.syms.def_results.get(&sym) {
            Some(&t) => t,
            None => self.u.intern_type(Type::Error),
        }
    }
}
