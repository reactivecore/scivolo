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

    fn is_param_or_accessor(&self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        match self.syms.flags.get(&s) {
            Some(flags) => flags.is(f::PARAM) || flags.is(f::PARAM_ACCESSOR),
            None => sym.kind == Kind::Param || sym.flags.has(PARAMsetter),
        }
    }

    /// Whether a reference of type `tpe` from `ctx` can be an identifier, its prefix implicit.
    pub fn prefix_is_elidable(&mut self, tpe: TypeId, ctx: Ctx) -> bool {
        let Type::Sym(prefix, sym) = *self.u.types().get(tpe) else { return true };
        if self.is_type_symbol(sym) || self.u.symbol(sym).flags.has(STATIC) {
            return true;
        }
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
    fn is_static_term(&self, s: SymbolId) -> bool {
        self.u.symbol(s).owner.is_none_or(|o| self.is_static_owner(o))
    }

    /// `C.this`, of class type `class` at `span`, its qualifier the untyped name of the class
    /// (dotty's `tpd.This(cls)`).
    pub fn this_tree(&mut self, class: TypeId, span: Span) -> TNodeId {
        let cls = self.u.class_symbol(class);
        let text = cls.map(|c| self.u.symbol(c).name.clone()).unwrap_or_default();
        let qual = TName { name: self.name(&text), is_type: true };
        let qual = self.tpd.node(T::Ident { name: qual }, span.start_pos(), None, None);
        let tpe = self.u.intern_type(Type::This(class));
        self.tpd.node(T::This { qual }, span.start_pos(), Some(tpe), cls)
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
        let mut cls = self.enclosing_class(ctx.owner);
        if let Some(n) = name {
            let text = self.text(n.name).to_owned();
            let named = |u: &tasty::Universe, c: SymbolId| {
                let s = u.symbol(c);
                s.name == text
                    || s.flags.has(tasty::tags::OBJECT) && s.name.strip_suffix('$') == Some(text.as_str())
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
        let this = self.this_tree(class, span);
        self.tpd.get_mut(this).span = span;
        Some(this)
    }

    /// `C.this.name` of type `tpe`, a reference to `sym` at `span` (dotty's `ref(tpe)`).
    pub fn this_select(&mut self, span: Span, name: TName, tpe: TypeId, sym: SymbolId) -> TNodeId {
        let Type::Sym(Some(prefix), _) = *self.u.types().get(tpe) else { unreachable!("a member reference") };
        let qualifier = match *self.u.types().get(prefix) {
            Type::This(class) => self.this_tree(class, span),
            _ => {
                self.unsupported_at(span, "reference through a path");
                return self.tpd.node(T::Ident { name }, span, Some(tpe), Some(sym));
            }
        };
        self.tpd.node(T::Select { qualifier, name }, span, Some(tpe), Some(sym))
    }
}
