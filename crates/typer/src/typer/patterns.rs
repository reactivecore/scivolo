//! Typed patterns, binds, variables and wildcards (dotty's `typedTyped`, `typedBind`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags::{CASE, GIVEN};
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
        let pattern = ctx.mode.is(Mode::PATTERN);
        let tpt_id = tpt;
        let tpt = self.typed_type(tpt, ctx.with_mode(if pattern { Mode::PATTERN_TYPE } else { Mode::NONE }));
        let tpe = self.tpd.tpe(tpt);
        if pattern && !self.is_testable_type(tpe) {
            match self.try_with_type_test(id, expr, tpt_id, tpe, pt, ctx) {
                Ok(Some(tagged)) => return tagged,
                Ok(None) if self.is_erased_test_type(tpe) => {
                    self.warn_unchecked_type_test(pt, tpe, self.ast.span(id));
                }
                Ok(None) => return self.unsupported(id, "type pattern needing a type test or type binding"),
                Err(what) => return self.unsupported(id, what),
            }
        }
        let expr = self.typed(expr, Pt::Type(tpe), ctx);
        let span = self.ast.span(id);
        self.tpd.node(T::Typed { expr, tpt }, span, Some(tpe), None)
    }

    pub fn typed_bind(&mut self, id: NodeId, name: TName, body: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let pt = match pt {
            Pt::Type(p) if !self.is_fully_defined(p) => match self.fully_defined_as(p, true) {
                Ok(p) => Pt::Type(p),
                Err(what) => return self.unsupported(id, what),
            },
            pt => pt,
        };
        let ref_or_literal = matches!(
            self.ast.tree(body),
            Tree::Ident { .. } | Tree::Select { .. } | Tree::Literal { .. } | Tree::Number { .. }
        );
        let given =
            matches!(*self.ast.tree(id), Tree::Bind { mods, .. } if self.ast.modifiers(mods).is(GIVEN));
        let given_tpt = match *self.ast.tree(body) {
            Tree::Typed { tpt, .. } if given => Some(tpt),
            _ => None,
        };
        let body = self.typed(body, pt, ctx);
        let mut text = self.text(name.name).to_owned();
        if text == "_"
            && let Some(tpt) = given_tpt
        {
            text = syntax::desugar::given_type_name(self.ast, self.names, tpt);
        }
        if text == "_" {
            return body;
        }
        if !given && let Some(tagged) = self.bind_in_type_test(id, body, ctx) {
            return tagged;
        }
        let name = TName { name: self.name(&text), is_type: false };
        let body_tpe = self.tpd.tpe(body);
        let stable_or_literal = match *self.tpd.tree(body) {
            T::Ident { name } => self.text(name.name) != "_",
            T::Select { .. } | T::Literal { .. } => true,
            _ => false,
        };
        let implicit = matches!(pt, Pt::Type(p) if Some(p) == self.syms.implicit_scrutinee);
        let info = match pt {
            _ if !ref_or_literal && self.is_error(body) => body_tpe,
            _ if implicit => body_tpe,
            Pt::Type(p) if stable_or_literal => p,
            Pt::Type(p) if self.binds_tuple_pt(body_tpe, p) => p,
            Pt::Type(p) if !self.is_bottom_type(p) && !self.u.is_subtype(body_tpe, p) => {
                self.u.glb(body_tpe, p)
            }
            _ => body_tpe,
        };
        let flags = if implicit || given { CASE.or(GIVEN) } else { CASE };
        let flags = flags.to_term_flags();
        let sym = self.syms.define(self.u, &text, Kind::Val, ctx.owner, flags);
        self.u.set_info(sym, info);
        self.levels.enter(ctx.level, &text, sym);
        let tpe = self.u.intern_type(Type::Sym(None, sym));
        let span = self.ast.span(id);
        self.tpd.node(T::Bind { name, body }, span, Some(tpe), Some(sym))
    }

    /// Whether `t` is `Nothing` or `Null` (dotty's `isBottomType`).
    pub fn is_bottom_type(&mut self, t: TypeId) -> bool {
        let b = self.u.builtins();
        let (nothing, null) = (b.nothing, b.null);
        matches!(self.u.class_symbol(t), Some(c) if c == nothing || c == null)
    }

    /// Whether `t` is a class type, possibly applied, without wildcard arguments.
    pub fn is_checkable_class_type(&mut self, t: TypeId) -> bool {
        self.is_class_type(t, false)
    }

    /// Whether a typed pattern can test for `t`: a class type, possibly applied, a singleton
    /// type, or a union or intersection of them.
    pub fn is_testable_type(&mut self, t: TypeId) -> bool {
        let t = self.dealiased(t);
        match *self.u.types().get(t) {
            Type::And(a, b) | Type::Or(a, b, _) => self.is_testable_type(a) && self.is_testable_type(b),
            Type::TermRef(..) | Type::This(_) | Type::Constant(..) | Type::Str(_) => true,
            Type::Sym(_, s) if !self.u.symbol(s).kind.is_type() => true,
            _ => self.is_class_type(t, true),
        }
    }

    /// `t` with its aliases, also those of script symbols, followed.
    pub fn dealiased(&mut self, t: TypeId) -> TypeId {
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
        t
    }

    /// Whether `t` is a class type, possibly applied, with wildcard arguments if `wildcards`.
    pub fn is_class_type(&mut self, t: TypeId, wildcards: bool) -> bool {
        let t = self.dealiased(t);
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
        let sym = match self.u.types().get(self.stripped(tpe)).clone() {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, name) => self.u.resolve(p, &name, true),
            Type::TermRef(p, name) => self.u.resolve(p, &name, false),
            Type::This(c) => self.u.class_symbol(c),
            _ => None,
        };
        let span = self.ast.span(id);
        self.tpd.node(T::Ident { name }, span, Some(tpe), sym)
    }
}
