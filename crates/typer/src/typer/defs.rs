// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
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
            None => self.typed_tpt(tpt, info, rhs_ctx),
        };
        let pt = info.map_or(Pt::Any, Pt::Type);
        let rhs = rhs.map(|r| match *self.ast.tree(r) {
            Tree::Ident { name } if self.text(name.name) == "_" => self.default_initializer(id, r, sym, info),
            _ => self.typed(r, pt, rhs_ctx),
        });
        let span = self.ast.span(id);
        let inline = sym.is_some_and(|s| {
            let s = self.u.symbol(s);
            s.flags.has(tasty::tags::INLINE) && s.kind == tasty::Kind::Val
        });
        let constant =
            info.is_some_and(|t| matches!(self.u.types().get(t), Type::Constant(..) | Type::Str(_)));
        let in_inline_method = std::iter::successors(Some(ctx.owner), |&o| self.u.symbol(o).owner).any(|o| {
            self.u.symbol(o).kind == tasty::Kind::Def && self.u.symbol(o).flags.has(tasty::tags::INLINE)
        });
        if inline && !in_inline_method && (!constant || rhs.is_some_and(|r| !self.is_inline_value_rhs(r))) {
            return self.unsupported(id, "inline value of a type that is not constant or impure");
        }
        self.tpd.node(T::ValDef { name, tpt, rhs }, span, None, sym)
    }

    /// The `_` right-hand side `rhs` of variable field `vdef`, of the field's type `info`.
    fn default_initializer(
        &mut self,
        vdef: NodeId,
        rhs: NodeId,
        sym: Option<SymbolId>,
        info: Option<tasty::TypeId>,
    ) -> TNodeId {
        let Tree::Ident { name } = *self.ast.tree(rhs) else { unreachable!("a wildcard") };
        let field = sym.filter(|&s| {
            let class_member = self.u.symbol(s).owner.is_some_and(|o| self.u.is_class(o));
            class_member && self.syms.flags.get(&s).is_some_and(|f| f.is(syntax::ast::flags::MUTABLE))
        });
        match (field, info) {
            (Some(_), Some(tpe)) => {
                let named = match *self.u.types().get(tpe) {
                    Type::Sym(_, s) => Some(s),
                    _ => None,
                };
                self.tpd.node(T::Ident { name }, self.ast.span(rhs), Some(tpe), named)
            }
            _ => self.unsupported(vdef, "`_` other than as a variable field's value"),
        }
    }

    /// Type tree `tpt`, or, if the source gives none, an inferred one of type `info`.
    fn typed_tpt(&mut self, tpt: NodeId, info: Option<tasty::TypeId>, ctx: Ctx) -> TNodeId {
        if self.is_inferred_tpt(tpt) {
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
        let owner = self.u.symbol(sym).owner;
        if rhs.is_some() && self.u.symbol(sym).name == "<init>" && owner.is_some_and(|o| self.u.is_trait(o)) {
            self.error_at(self.ast.span(id), "Traits cannot have secondary constructors");
        }
        let erasure_mark = self.erasure_diags.items.len();
        let linked = self.link_constructor_params(sym, id);
        let mut rhs = rhs.map(|r| self.typed(r, Pt::Type(result), mctx));
        self.unlink_constructor_params(linked);
        if let Some(r) = rhs
            && self.u.symbol(sym).name == "<init>"
            && !self.calls_preceding_constructor(r, sym)
        {
            return self.unsupported(id, "secondary constructor calling a later constructor");
        }
        let flags = self.u.symbol(sym).flags;
        if flags.has(tasty::tags::INLINE) && self.u.symbol(sym).name == "<init>" {
            return self.unsupported(id, "inline constructor");
        }
        if flags.has(tasty::tags::INLINE) {
            self.erasure_diags.items.truncate(erasure_mark);
            if self.u.annotations(sym).iter().any(|&c| self.u.full_name(c) == "scala.annotation.tailrec") {
                return self.unsupported(id, "tailrec inline method");
            }
            if !flags.has(tasty::tags::TRANSPARENT) {
                rhs = rhs.map(|r| self.inline_body(r, tpt));
            }
            if let Some(r) = rhs {
                self.make_inlineable(r, sym, mctx);
            }
        }
        let span = self.ast.span(id);
        let def = self.tpd.node(T::DefDef { name, paramss, tpt, rhs }, span, None, Some(sym));
        if flags.has(tasty::tags::INLINE) {
            self.inline_defs.insert(sym, super::InlineDef::Typed(def));
        }
        def
    }

    /// Inline method body `rhs` ascribed its result type tree `tpt` (dotty's `wrapRHS`).
    fn inline_body(&mut self, rhs: TNodeId, tpt: TNodeId) -> TNodeId {
        let span = self.tpd.span(tpt).union(self.tpd.span(rhs)).to_synthetic();
        let tpe = self.tpd.tpe(tpt);
        self.tpd.node(T::Typed { expr: rhs, tpt }, span, Some(tpe), None)
    }

    /// Whether the self-constructor call in constructor body `tree` of `sym`, if any, calls a
    /// constructor defined before `sym` (dotty's `checkThisConstrCall`).
    fn calls_preceding_constructor(&mut self, tree: TNodeId, sym: SymbolId) -> bool {
        match *self.tpd.tree(tree) {
            T::Block { stats, expr } => {
                let first = self.tpd.items(stats).first().copied();
                first.is_none_or(|s| self.calls_preceding_constructor(s, sym))
                    && self.calls_preceding_constructor(expr, sym)
            }
            T::Apply { .. } => {
                let callee = self.tpd.meth_part(tree);
                let this_call = matches!(*self.tpd.tree(callee), T::Select { qualifier, .. }
                    if matches!(self.tpd.tree(qualifier), T::This { .. }));
                let Some(target) =
                    self.tpd.sym(callee).filter(|&s| this_call && self.u.symbol(s).name == "<init>")
                else {
                    return true;
                };
                let primary =
                    self.u.symbol(target).owner.and_then(|c| self.syms.primary_ctors.get(&c).copied());
                if primary == Some(target) {
                    return true;
                }
                let start = |t: &Self, s| t.syms.def_trees.get(&s).map(|&d| t.ast.span(d).start());
                match (start(self, sym), start(self, target)) {
                    (Some(own), Some(called)) => own > called || self.tpd.span(tree).is_synthetic(),
                    _ => true,
                }
            }
            _ => true,
        }
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
