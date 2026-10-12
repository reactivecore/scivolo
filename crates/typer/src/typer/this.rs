//! References to members of an enclosing class: `C.this.m` where the prefix cannot be left
//! implicit (dotty's `prefixIsElidable`, `ref`), and `this` itself.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{Span, TName};
use tasty::tags::{PARAMsetter, STATIC, TRAIT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The class enclosing `owner`, or `owner` itself if it is one.
    pub fn enclosing_class(&self, owner: SymbolId) -> Option<SymbolId> {
        let mut o = Some(owner);
        while let Some(s) = o {
            if self.u.symbol(s).kind == Kind::Class {
                return Some(s);
            }
            o = self.u.symbol(s).owner;
        }
        None
    }

    /// The class `this` refers to in `owner`: the enclosing class, or the class enclosing that
    /// one in its parent calls, which the primary constructor owns (dotty's `superCallContext`).
    fn this_class(&self, owner: SymbolId) -> Option<SymbolId> {
        let mut o = Some(owner);
        while let Some(s) = o {
            let sym = self.u.symbol(s);
            if sym.kind == Kind::Class {
                return Some(s);
            }
            if let Some(c) = sym.owner
                && self.syms.primary_ctors.get(&c) == Some(&s)
            {
                return self.u.symbol(c).owner.and_then(|oc| self.enclosing_class(oc));
            }
            o = sym.owner;
        }
        None
    }

    pub fn is_param_or_accessor(&self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        match self.syms.flags.get(&s) {
            Some(flags) => flags.is(f::PARAM) || flags.is(f::PARAM_ACCESSOR),
            None => sym.kind == Kind::Param || sym.flags.has(PARAMsetter),
        }
    }

    /// Whether a reference of type `tpe` from `ctx` can be an identifier, its prefix implicit.
    pub fn prefix_is_elidable(&mut self, tpe: TypeId, ctx: Ctx) -> bool {
        let Type::Sym(prefix, sym) = *self.u.types().get(tpe) else { return true };
        if self.u.symbol(sym).flags.has(STATIC) {
            return true;
        }
        if self.u.is_type_symbol(sym) {
            return match prefix.map(|p| self.u.types().get(p).clone()) {
                Some(Type::Sym(_, s)) => {
                    self.u.symbol(s).flags.has(tasty::tags::OBJECT) && self.is_static_term(s)
                }
                Some(Type::TermRef(..)) => false,
                _ => true,
            };
        }
        self.term_prefix_is_elidable(prefix, sym, ctx)
    }

    /// Whether a reference to term `sym` through `prefix` from `ctx` can be an identifier.
    pub fn term_prefix_is_elidable(&mut self, prefix: Option<TypeId>, sym: SymbolId, ctx: Ctx) -> bool {
        let Some(prefix) = prefix else { return true };
        match *self.u.types().get(prefix) {
            Type::This(c) => {
                let Some(cls) = self.u.class_symbol(c) else { return true };
                let owner_is_trait =
                    self.u.symbol(sym).owner.is_some_and(|o| self.u.symbol(o).flags.has(TRAIT));
                self.is_static_owner(cls)
                    || self.is_param_or_accessor(sym)
                        && !self.u.symbol(cls).flags.has(TRAIT)
                        && !owner_is_trait
                        && self.enclosing_class(ctx.owner) == Some(cls)
            }
            Type::Sym(_, s) => self.u.symbol(s).flags.has(tasty::tags::OBJECT) && self.is_static_term(s),
            _ => true,
        }
    }

    /// Whether module value `s` is reachable from the root through objects.
    pub fn is_static_term(&self, s: SymbolId) -> bool {
        self.u.symbol(s).owner.is_none_or(|o| self.is_static_owner(o))
    }

    /// `this` or `C.this` as written (dotty's `typedThis`): the innermost enclosing class, or
    /// the one named `C`.
    pub fn typed_this(
        &mut self,
        id: syntax::ast::NodeId,
        qual: Option<syntax::ast::NodeId>,
        ctx: Ctx,
    ) -> TNodeId {
        let name = match qual.map(|q| self.ast.tree(q).clone()) {
            Some(syntax::ast::Tree::Ident { name }) => Some(name),
            Some(_) => return self.unsupported(id, "this qualifier"),
            None => None,
        };
        if ctx.mode.is(crate::ctx::Mode::THIS_CALL_ARGS) {
            return self.unsupported(id, "this in the arguments of a self-constructor call");
        }
        let mut cls = self.this_class(ctx.owner);
        if let Some(n) = name {
            let text = self.text(n.name).to_owned();
            let named = |u: &tasty::Universe, c: SymbolId| {
                let s = u.symbol(c);
                s.name == text
                    || s.flags.has(tasty::tags::OBJECT) && tasty::module_name(&s.name) == Some(text.as_str())
            };
            while let Some(c) = cls.filter(|&c| !named(self.u, c)) {
                cls = self.u.symbol(c).owner.and_then(|o| self.enclosing_class(o));
            }
        }
        let Some(cls) = cls else { return self.unsupported(id, "this outside a class") };
        let span = self.ast.span(id);
        let qual = match (qual, name) {
            (Some(q), Some(n)) => {
                let qspan = self.ast.span(q);
                self.tpd.node(T::Ident { name: n }, qspan, None, None)
            }
            _ => {
                let empty = TName { name: self.name(""), is_type: true };
                self.tpd.node(T::Ident { name: empty }, Span::NONE, None, None)
            }
        };
        let class = crate::defn::type_ref(self.u, cls);
        let tpe = self.u.intern_type(Type::This(class));
        self.tpd.node(T::This { qual }, span, Some(tpe), Some(cls))
    }

    /// `C.this` at `span` for a reference to module `sym` from inside its class `C`.
    pub fn module_this(&mut self, sym: SymbolId, span: Span, ctx: Ctx) -> Option<TNodeId> {
        if !self.u.symbol(sym).flags.has(tasty::tags::OBJECT) {
            return None;
        }
        let module_class = self.u.module_class(sym)?;
        let mut o = Some(ctx.owner);
        while let Some(s) = o.filter(|&s| s != module_class) {
            o = self.u.symbol(s).owner;
        }
        o?;
        let class = crate::defn::type_ref(self.u, module_class);
        let this = self.build().this(class, span);
        self.tpd.get_mut(this).span = span;
        Some(this)
    }

    /// A reference of type `tpe` to member `sym` of `prefix` from `ctx` by its term name (dotty's
    /// `ref(tp)`): an identifier if the prefix can be left implicit, else `C.this.name`.
    pub fn ref_(
        &mut self,
        prefix: Option<TypeId>,
        sym: SymbolId,
        tpe: TypeId,
        span: Span,
        ctx: Ctx,
    ) -> TNodeId {
        if self.term_prefix_is_elidable(prefix, sym, ctx) {
            return self.build().ident(sym, tpe, span);
        }
        let name = self.build().term_name(sym);
        self.this_select(span, name, tpe, sym)
    }

    /// `p.name` of type `tpe`, a reference to `sym` at `span` through the path `p` of its prefix
    /// (dotty's `ref(tpe)`).
    pub fn this_select(&mut self, span: Span, name: TName, tpe: TypeId, sym: SymbolId) -> TNodeId {
        let Type::Sym(Some(prefix), _) = *self.u.types().get(tpe) else { unreachable!("a member reference") };
        let Some(qualifier) = self.path_tree(prefix, span.start_pos(), 8) else {
            self.unsupported_at(span, "reference through a path");
            return self.tpd.node(T::Ident { name }, span, Some(tpe), Some(sym));
        };
        let name = match self.u.types().get(prefix) {
            Type::This(_) => name,
            _ => TName { is_type: name.is_type, ..self.build().term_name(sym) },
        };
        self.tpd.node(T::Select { qualifier, name }, span, Some(tpe), Some(sym))
    }
}
