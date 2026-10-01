//! The parents of a class: their types when it is completed (dotty's `ensureFirstIsClass`) and
//! their trees when it is typed (`parentTrees`, `ensureConstrCall`).

use super::classes::Template;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The type tree of a parent: the class of a constructor call, or the parent itself.
    fn parent_tpt(&self, parent: NodeId) -> NodeId {
        match *self.ast.tree(parent) {
            Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } => self.parent_tpt(fun),
            Tree::Select { qualifier, name } if self.text(name.name) == "<init>" => {
                self.parent_tpt(qualifier)
            }
            Tree::New { tpt } => tpt,
            _ => parent,
        }
    }

    pub fn is_trait(&self, c: SymbolId) -> bool {
        self.u.symbol(c).flags.has(tasty::tags::TRAIT)
    }

    /// The parent types of a class from parent trees `parents`, typed in `ctx`, with a class
    /// first.
    pub fn parent_types(&mut self, parents: &[NodeId], ctx: Ctx) -> Vec<TypeId> {
        let mut types = Vec::new();
        for &p in parents {
            let tpt = self.parent_tpt(p);
            let typed = self.typed_type(tpt, ctx);
            let tpe = self.tpd.tpe(typed);
            if let Some(c) = self.u.class_symbol(tpe) {
                self.complete(c);
                if self.ast.span(p).is_source_derived()
                    && types.iter().any(|&t| self.u.class_symbol(t) == Some(c))
                {
                    self.unsupported(p, "class extended twice");
                }
            }
            types.push(tpe);
        }
        self.ensure_first_is_class(types)
    }

    /// `parents` with the class parent their class parents agree on first, if they do not start
    /// with a class (dotty's `ensureFirstIsClass`).
    fn ensure_first_is_class(&mut self, parents: Vec<TypeId>) -> Vec<TypeId> {
        let first = parents.first().and_then(|&p| self.u.class_symbol(p));
        if first.is_some_and(|c| !self.is_trait(c)) {
            return parents;
        }
        let mut pcls = self.defn.object;
        for &p in &parents {
            let class = self.u.class_symbol(p);
            let candidate = self.real_class_parent(class);
            if self.u.derives_from(candidate, pcls) {
                pcls = candidate;
            }
        }
        let object = type_ref(self.u, self.defn.object);
        let mut first = object;
        for &p in &parents {
            if let Some(base) = self.u.base_type(p, pcls) {
                first = self.u.glb(first, base);
            }
        }
        let mut out = vec![first];
        out.extend(parents);
        out
    }

    /// The class `sym` is or, for a trait, the class its first parent leads to.
    fn real_class_parent(&mut self, sym: Option<SymbolId>) -> SymbolId {
        let Some(sym) = sym.filter(|&s| self.u.symbol(s).kind == Kind::Class) else {
            return self.defn.object;
        };
        if !self.is_trait(sym) {
            return sym;
        }
        self.complete(sym);
        let first = self.u.parents(sym).first().copied();
        let parent = first.and_then(|p| self.u.class_symbol(p));
        match parent {
            Some(_) => self.real_class_parent(parent),
            None => self.defn.object,
        }
    }

    /// The parent trees of class `cls`: those of `parents` typed in `ctx`, and calls for the parent
    /// types the completer added, at the class name `name_span` (dotty's `parentTrees`).
    pub fn parent_trees(&mut self, cls: SymbolId, t: &Template, ctx: Ctx, name_span: Span) -> Vec<TNodeId> {
        let (parents, super_ctx) = (&t.parents, self.super_call_ctx(t.constr, ctx));
        let trees: Vec<TNodeId> = parents.iter().map(|&p| self.typed_parent(cls, p, super_ctx)).collect();
        let types = self.u.parents(cls);
        let mut out = Vec::new();
        let mut rest = trees.as_slice();
        for t in types {
            let psym = self.u.class_symbol(t);
            if let Some(k) = rest.iter().position(|&tree| self.parent_class(tree) == psym) {
                out.extend_from_slice(&rest[..=k]);
                rest = &rest[k + 1..];
            } else {
                let tpt = self.type_tree(t, name_span, false);
                out.push(self.ensure_constr_call(cls, tpt));
            }
        }
        out.extend_from_slice(rest);
        out
    }

    /// The class a parent tree extends: a constructor call's class, or its type's.
    fn parent_class(&mut self, tree: TNodeId) -> Option<SymbolId> {
        let mut fun = tree;
        while let T::Apply { fun: f, .. } = *self.tpd.tree(fun) {
            fun = f;
        }
        match self.tpd.sym(fun) {
            Some(ctor) if self.u.symbol(ctor).name == "<init>" => self.u.symbol(ctor).owner,
            _ => {
                let tpe = self.tpd.tpe(tree);
                self.u.class_symbol(tpe)
            }
        }
    }

    /// The context of a class's parent calls, defined in `ctx`: owned by its primary constructor
    /// `constr`, seeing the class's type parameters and parameter accessors (dotty's
    /// `superCallContext`).
    fn super_call_ctx(&mut self, constr: NodeId, ctx: Ctx) -> Ctx {
        let Some(&ctor) = self.syms.by_tree.get(&constr) else { return ctx };
        let Some(cls) = self.u.symbol(ctor).owner else { return ctx };
        use syntax::ast::flags::{PARAM_ACCESSOR, TYPE_PARAM};
        let locals: Vec<(String, SymbolId)> = self
            .u
            .symbol(cls)
            .decls
            .iter()
            .filter(|d| self.syms.flags.get(d).is_some_and(|f| f.is(TYPE_PARAM) || f.is(PARAM_ACCESSOR)))
            .map(|&d| (self.u.symbol(d).name.clone(), d))
            .collect();
        let level = self.levels.push(Some(ctx.level), crate::ctx::Level::Scope(locals));
        Ctx { owner: ctor, level, ..ctx }
    }

    /// A parent: a constructor call, or a type made one if it is a class's parent class.
    fn typed_parent(&mut self, cls: SymbolId, parent: NodeId, ctx: Ctx) -> TNodeId {
        if matches!(self.ast.tree(parent), Tree::Apply { .. }) {
            return self.typed(parent, Pt::Any, ctx);
        }
        let tpt = self.typed_type(parent, ctx);
        self.ensure_constr_call(cls, tpt)
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
