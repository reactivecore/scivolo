//! `try` with cases, and the patterns of its cases: typed patterns, binds, wildcards (dotty's
//! `typedTry`, `typedCase`, `typedTyped`, `typedBind`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{List, NodeId, TName, Tree};
use syntax::names::is_var_pattern_name;
use tasty::{Kind, Type};

impl Typer<'_> {
    pub fn typed_try(&mut self, id: NodeId, expr: NodeId, cases: List, pt: Pt, ctx: Ctx) -> TNodeId {
        let expr = self.typed(expr, pt, ctx);
        let throwable = crate::defn::type_ref(self.u, self.defn.throwable);
        let case_ids = self.ast.items(cases).to_vec();
        let typed_cases: Vec<TNodeId> =
            case_ids.iter().map(|&c| self.typed_case_def(c, throwable, pt, ctx)).collect();
        let mut types = vec![self.tpd.tpe(expr)];
        for &c in &typed_cases {
            let T::CaseDef { body, .. } = *self.tpd.tree(c) else { unreachable!() };
            types.push(self.tpd.tpe(body));
        }
        let first = types[0];
        if types.iter().any(|&t| !(self.u.is_subtype(t, first) && self.u.is_subtype(first, t))) {
            return self.unsupported(id, "least upper bound");
        }
        let cases = self.tpd.list(&typed_cases);
        let span = self.ast.span(id);
        self.tpd.node(T::Try { expr, cases, finalizer: None }, span, Some(types[0]), None)
    }

    fn typed_case_def(&mut self, id: NodeId, selector: tasty::TypeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let Tree::CaseDef { pat, guard, body } = *self.ast.tree(id) else {
            return self.unsupported(id, "case");
        };
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        let cctx = Ctx { level, ..ctx };
        let pat = self.typed(pat, Pt::Type(selector), cctx.with_mode(ctx.mode.with(Mode::PATTERN)));
        let guard = guard.map(|g| self.typed(g, Pt::Any, cctx));
        let body = self.typed(body, pt, cctx);
        let span = self.ast.span(id);
        self.tpd.node(T::CaseDef { pat, guard, body }, span, None, None)
    }

    pub fn typed_typed(&mut self, id: NodeId, expr: NodeId, tpt: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        if ctx.mode.is(Mode::PATTERN)
            && let Tree::Ident { name } = *self.ast.tree(expr)
            && self.text(name.name) != "_"
            && is_var_pattern_name(self.text(name.name))
            && !self.ast.attrs(expr).contains(syntax::ast::Attrs::BACKQUOTED)
        {
            let wild = TName { name: self.name("_"), is_type: false };
            let wild = self.ast.node(Tree::Ident { name: wild });
            let typed = self.ast.node(Tree::Typed { expr: wild, tpt });
            let mods = self.ast.mods(Modifiers::default());
            let bind = self.ast.node(Tree::Bind { name, body: typed, mods });
            self.ast.with_span(bind, self.ast.span(id));
            return self.typed(bind, pt, ctx);
        }
        let tpt = self.typed_type(tpt, ctx);
        let tpe = self.tpd.tpe(tpt);
        let expr = self.typed(expr, Pt::Type(tpe), ctx);
        let span = self.ast.span(id);
        self.tpd.node(T::Typed { expr, tpt }, span, Some(tpe), None)
    }

    pub fn typed_bind(&mut self, id: NodeId, name: TName, body: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let body = self.typed(body, pt, ctx);
        let text = self.text(name.name).to_owned();
        let flags = syntax::ast::flags::CASE.to_term_flags();
        let sym = self.syms.define(self.u, &text, Kind::Val, ctx.owner, flags);
        let info = self.tpd.tpe(body);
        self.u.set_info(sym, info);
        self.levels.enter(ctx.level, &text, sym);
        let tpe = self.u.intern_type(Type::Sym(None, sym));
        let span = self.ast.span(id);
        self.tpd.node(T::Bind { name, body }, span, Some(tpe), Some(sym))
    }

    /// `_` as a pattern: of the expected type, with that type's class as its symbol.
    pub fn wildcard_pattern(&mut self, id: NodeId, name: TName, pt: Pt) -> TNodeId {
        let Pt::Type(tpe) = pt else { return self.unsupported(id, "wildcard without expected type") };
        let sym = self.u.class_symbol(tpe);
        let span = self.ast.span(id);
        self.tpd.node(T::Ident { name }, span, Some(tpe), sym)
    }
}
