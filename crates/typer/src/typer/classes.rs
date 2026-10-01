//! Class definitions: parents, constructor, members (dotty's `ClassCompleter`, `typedClassDef`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level};
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

/// The parts of a template.
struct Template {
    constr: NodeId,
    parents: Vec<NodeId>,
    self_: Option<NodeId>,
    body: Vec<NodeId>,
}

impl Typer<'_> {
    fn template(&self, rhs: NodeId) -> Template {
        let Tree::Template { constr, parents_or_derived, self_, body, derived } = *self.ast.tree(rhs) else {
            unreachable!("a class definition's template")
        };
        let parents = self.ast.items(parents_or_derived);
        let parents = parents[..parents.len() - derived.unwrap_or(0) as usize].to_vec();
        Template { constr, parents, self_, body: self.ast.items(body).to_vec() }
    }

    fn class_ctx(&mut self, cls: SymbolId, ctx: Ctx) -> Ctx {
        let level = self.levels.push(Some(ctx.level), Level::Members(cls));
        Ctx { owner: cls, level, ..ctx }
    }

    /// The type tree of a parent: the class of a constructor call, or the parent itself.
    fn parent_tpt(&self, parent: NodeId) -> NodeId {
        match *self.ast.tree(parent) {
            Tree::Apply { fun, .. } => self.parent_tpt(fun),
            Tree::Select { qualifier, .. } => self.parent_tpt(qualifier),
            Tree::New { tpt } => tpt,
            _ => parent,
        }
    }

    /// Sets the parents of class `cls` and enters its constructor and members.
    pub fn complete_class(&mut self, cls: SymbolId, rhs: NodeId, ctx: Ctx) {
        let t = self.template(rhs);
        let mut parents: Vec<TypeId> = Vec::new();
        for &p in &t.parents {
            let tpt = self.parent_tpt(p);
            let typed = self.typed_type(tpt, ctx);
            parents.push(self.tpd.tpe(typed));
        }
        if parents.is_empty() {
            parents.push(type_ref(self.u, self.defn.object));
        }
        for &p in &parents {
            if let Some(c) = self.u.class_symbol(p) {
                self.complete(c);
            }
        }
        let any_val = self.u.builtins().any_val;
        if any_val.is_some() && parents.iter().any(|&p| self.u.class_symbol(p) == any_val) {
            self.unsupported(rhs, "value class");
        }
        let first = self.u.class_symbol(parents[0]);
        if !self.is_trait(cls) && first.is_none_or(|c| self.is_trait(c)) {
            self.unsupported(rhs, "a trait as first parent");
        }
        if let Some(s) = t.self_
            && !self.is_module_self(s)
        {
            self.unsupported(s, "self type");
        }
        self.u.set_parents(cls, parents);
        let no_inits = self.has_no_inits(&t.parents, &t.body);
        self.syms.no_inits.insert(cls, no_inits);
        let cctx = self.class_ctx(cls, ctx);
        let mut members = vec![t.constr];
        members.extend(&t.body);
        self.index(&members, cctx);
        self.set_stable_constructor(cls, t.constr);
    }

    pub fn typed_class_def(&mut self, id: NodeId, ctx: Ctx) -> TNodeId {
        let Tree::TypeDef { name, rhs, .. } = *self.ast.tree(id) else { unreachable!() };
        let Tree::Template { .. } = self.ast.tree(rhs) else {
            return self.unsupported(id, "type definition");
        };
        let cls = self.syms.by_tree[&id];
        self.complete(cls);
        let t = self.template(rhs);
        let cctx = self.class_ctx(cls, ctx);
        let constr = self.typed(t.constr, Pt::Any, cctx);
        let parents: Vec<TNodeId> = if t.parents.is_empty() {
            let object = type_ref(self.u, self.defn.object);
            let name_span = Span::at(self.ast.span(id).point());
            let tpt = self.type_tree(object, name_span, false);
            vec![self.ensure_constr_call(cls, tpt)]
        } else {
            t.parents.iter().map(|&p| self.typed_parent(cls, p, ctx)).collect()
        };
        let self_ = t.self_.map(|s| self.typed(s, Pt::Any, cctx));
        let body = self.typed_template_stats(cls, &t.body, cctx);
        let parents = self.tpd.list(&parents);
        let body = self.tpd.list(&body);
        let template = T::Template { constr, parents, self_, body };
        let template = self.tpd.node(template, self.ast.span(rhs), None, Some(cls));
        let span = self.ast.span(id);
        self.tpd.node(T::TypeDef { name, rhs: template }, span, None, Some(cls))
    }

    /// The statements of class `cls`'s body; expressions owned by the class's local dummy.
    fn typed_template_stats(&mut self, cls: SymbolId, stats: &[NodeId], mut ctx: Ctx) -> Vec<TNodeId> {
        let mut dummy = None;
        let mut out = Vec::new();
        for &s in stats {
            let stat_ctx = match self.ast.tree(s) {
                Tree::Empty => continue,
                Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => ctx,
                _ => {
                    let name = format!("<local {}>", self.u.symbol(cls).name);
                    let d = *dummy.get_or_insert_with(|| {
                        self.syms.define(self.u, &name, Kind::Val, cls, syntax::ast::flags::EMPTY)
                    });
                    ctx.with_owner(d)
                }
            };
            out.push(self.typed(s, Pt::Any, stat_ctx));
            ctx = self.after_stat(s, ctx);
        }
        out
    }

    /// A parent: a constructor call, or a type made one if it is a class's parent class.
    fn typed_parent(&mut self, cls: SymbolId, parent: NodeId, ctx: Ctx) -> TNodeId {
        if matches!(self.ast.tree(parent), Tree::Apply { .. }) {
            return self.typed(parent, Pt::Any, ctx);
        }
        let tpt = self.typed_type(parent, ctx);
        self.ensure_constr_call(cls, tpt)
    }

    fn is_trait(&self, c: SymbolId) -> bool {
        self.u.symbol(c).flags.has(tasty::tags::TRAIT)
    }

    /// Whether self definition `s` is the `_: O.type` desugaring gives an object's class.
    fn is_module_self(&self, s: NodeId) -> bool {
        matches!(*self.ast.tree(s), Tree::ValDef { name, tpt, .. }
            if self.text(name.name) == "_" && matches!(self.ast.tree(tpt), Tree::SingletonTypeTree { .. }))
    }

    /// The constructor of class `c` that takes one empty parameter list, if it has one.
    fn nullary_constructor(&mut self, c: SymbolId) -> Option<SymbolId> {
        let ctors = self.u.decl(c, "<init>").unwrap_or_default();
        ctors.into_iter().find(|&ctor| {
            self.complete(ctor);
            let info = self.u.info(ctor);
            matches!(self.u.types().get(info), Type::Method { params, result, .. }
                if params.is_empty() && !matches!(self.u.types().get(*result), Type::Method { .. } | Type::Poly { .. }))
        })
    }

    /// `new P()` for parent type tree `parent` of class `cls` if `P` is a class and `cls` is not
    /// a trait (dotty's `ensureConstrCall`).
    fn ensure_constr_call(&mut self, cls: SymbolId, parent: TNodeId) -> TNodeId {
        let tpe = self.tpd.tpe(parent);
        let Some(psym) = self.u.class_symbol(tpe) else { return parent };
        if self.is_trait(cls) || self.is_trait(psym) {
            return parent;
        }
        let span = self.tpd.span(parent).to_synthetic();
        let Some(ctor) = self.nullary_constructor(psym) else {
            self.unsupported_at(span, "parent constructor with arguments");
            return parent;
        };
        let new = self.tpd.node(T::New { tpt: parent }, span, Some(tpe), None);
        let sel_tpe = self.u.intern_type(Type::Sym(Some(tpe), ctor));
        let name = TName { name: self.name("<init>"), is_type: false };
        let sel = self.tpd.node(T::Select { qualifier: new, name }, span, Some(sel_tpe), Some(ctor));
        let args = self.tpd.list(&[]);
        self.tpd.node(T::Apply { fun: sel, args }, span, Some(tpe), None)
    }
}
