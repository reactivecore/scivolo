//! Typed patterns, binds, variables and wildcards (dotty's `typedTyped`, `typedBind`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use syntax::names::is_var_pattern_name;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    pub fn typed_typed(&mut self, id: NodeId, expr: NodeId, tpt: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        if ctx.mode.is(Mode::PATTERN)
            && let Tree::Ident { name } = *self.ast.tree(expr)
            && self.is_var_pattern(expr, self.text(name.name))
        {
            let (outer, inner) = (self.ast.span(id), self.ast.span(tpt));
            let parens = outer.end().saturating_sub(inner.end());
            let start = inner.start().saturating_sub(parens);
            let wild = TName { name: self.name("_"), is_type: false };
            let wild = self.ast.node(Tree::Ident { name: wild });
            self.ast.with_span(wild, Span::at(start));
            let typed = self.ast.node(Tree::Typed { expr: wild, tpt });
            self.ast.with_span(typed, Span::synthetic(start, outer.end()));
            let mods = self.ast.mods(Modifiers::default());
            let bind = self.ast.node(Tree::Bind { name, body: typed, mods });
            self.ast.with_span(bind, self.ast.span(id));
            return self.typed(bind, pt, ctx);
        }
        let tpt = self.typed_type(tpt, ctx.with_mode(Mode::NONE));
        let tpe = self.tpd.tpe(tpt);
        if ctx.mode.is(Mode::PATTERN) && !self.is_checkable_type(tpe) {
            return self.unsupported(id, "type pattern needing a type test or type binding");
        }
        if ctx.mode.is(Mode::PATTERN) && self.tests_value_class(tpe) {
            return self.unsupported(id, "type test or cast to a value class");
        }
        let expr = self.typed(expr, Pt::Type(tpe), ctx);
        let span = self.ast.span(id);
        self.tpd.node(T::Typed { expr, tpt }, span, Some(tpe), None)
    }

    pub fn typed_bind(&mut self, id: NodeId, name: TName, body: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let ref_or_literal = matches!(
            self.ast.tree(body),
            Tree::Ident { .. } | Tree::Select { .. } | Tree::Literal { .. } | Tree::Number { .. }
        );
        let body = self.typed(body, pt, ctx);
        let text = self.text(name.name).to_owned();
        if text == "_" {
            return body;
        }
        let body_tpe = self.tpd.tpe(body);
        let stable_or_literal = match *self.tpd.tree(body) {
            T::Ident { name } => self.text(name.name) != "_",
            T::Select { .. } | T::Literal { .. } => true,
            _ => false,
        };
        let b = self.u.builtins();
        let (nothing, null) = (b.nothing, b.null);
        let bottom = |t: &mut Self, p| matches!(t.u.class_symbol(p), Some(c) if c == nothing || c == null);
        let info = match pt {
            _ if !ref_or_literal && self.is_error(body) => body_tpe,
            Pt::Type(p) if stable_or_literal => p,
            Pt::Type(p) if !bottom(self, p) && !self.u.is_subtype(body_tpe, p) => {
                if !self.u.is_subtype(p, body_tpe) {
                    return self.unsupported(id, "pattern variable of an intersection type");
                }
                p
            }
            _ => body_tpe,
        };
        let flags = syntax::ast::flags::CASE.to_term_flags();
        let sym = self.syms.define(self.u, &text, Kind::Val, ctx.owner, flags);
        self.u.set_info(sym, info);
        self.levels.enter(ctx.level, &text, sym);
        let tpe = self.u.intern_type(Type::Sym(None, sym));
        let span = self.ast.span(id);
        self.tpd.node(T::Bind { name, body }, span, Some(tpe), Some(sym))
    }

    /// Whether `t` is a class type, possibly applied, without wildcard arguments.
    pub fn is_checkable_class_type(&mut self, t: TypeId) -> bool {
        self.is_class_type(t, false)
    }

    /// Whether `t` is a checkable class type or a union or intersection of them.
    fn is_checkable_type(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::And(a, b) | Type::Or(a, b) => self.is_checkable_type(a) && self.is_checkable_type(b),
            _ => self.is_checkable_class_type(t),
        }
    }

    /// Whether `t` is a class type, possibly applied, with wildcard arguments if `wildcards`.
    pub fn is_class_type(&mut self, t: TypeId, wildcards: bool) -> bool {
        let mut t = self.u.dealias(t);
        for _ in 0..8 {
            let Type::Sym(_, s) = *self.u.types().get(t) else { break };
            if self.u.symbol(s).kind != Kind::Type {
                break;
            }
            self.complete(s);
            let info = self.u.info(s);
            let Type::Alias(target) = *self.u.types().get(info) else { break };
            t = target;
        }
        let (tycon, args) = match self.u.types().get(t).clone() {
            Type::Applied(tc, args) => (tc, args),
            _ => (t, Vec::new()),
        };
        let sym = match self.u.types().get(tycon).clone() {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, name) => self.u.resolve(p, &name, true),
            _ => None,
        };
        let class = sym.is_some_and(|s| self.u.symbol(s).kind == Kind::Class);
        class && (wildcards || args.iter().all(|&a| !matches!(self.u.types().get(a), Type::Bounds(..))))
    }

    /// Whether identifier `id` named `text` is a variable pattern: a lower-case name, not
    /// back-quoted.
    pub fn is_var_pattern(&self, id: NodeId, text: &str) -> bool {
        text != "_"
            && is_var_pattern_name(text)
            && !self.ast.attrs(id).contains(syntax::ast::Attrs::BACKQUOTED)
    }

    /// Variable pattern `x` as `x @ _` (dotty's `patternVar`).
    pub fn pattern_var(&mut self, id: NodeId, name: TName, pt: Pt, ctx: Ctx) -> TNodeId {
        let span = self.ast.span(id);
        let wild = TName { name: self.name("_"), is_type: false };
        let wild = self.ast.node(Tree::Ident { name: wild });
        self.ast.with_span(wild, syntax::ast::Span::synthetic(span.start(), span.start()));
        let mods = self.ast.mods(Modifiers::default());
        let bind = self.ast.node(Tree::Bind { name, body: wild, mods });
        self.ast.with_span(bind, span);
        self.typed(bind, pt, ctx)
    }

    /// `_` as a pattern: of the expected type, with that type's class as its symbol.
    pub fn wildcard_pattern(&mut self, id: NodeId, name: TName, pt: Pt) -> TNodeId {
        let Pt::Type(tpe) = pt else { return self.unsupported(id, "wildcard without expected type") };
        let sym = match self.u.types().get(tpe).clone() {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, name) => self.u.resolve(p, &name, true),
            Type::TermRef(p, name) => self.u.resolve(p, &name, false),
            _ => None,
        };
        let span = self.ast.span(id);
        self.tpd.node(T::Ident { name }, span, Some(tpe), sym)
    }
}
