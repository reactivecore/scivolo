//! Class definitions: parents, constructor, members (dotty's `ClassCompleter`, `typedClassDef`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, Tree};
use tasty::{Kind, SymbolId};

/// The parts of a template.
pub struct Template {
    pub constr: NodeId,
    pub parents: Vec<NodeId>,
    pub self_: Option<NodeId>,
    pub body: Vec<NodeId>,
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

    /// Sets the parents of class `cls` and enters its constructor and members.
    pub fn complete_class(&mut self, cls: SymbolId, rhs: NodeId, ctx: Ctx) {
        let t = self.template(rhs);
        let cctx = self.class_ctx(cls, ctx);
        let mut members = vec![t.constr];
        members.extend(&t.body);
        self.index(&members, cctx);
        if let Some(&ctor) = self.syms.by_tree.get(&t.constr) {
            self.syms.primary_ctors.insert(cls, ctor);
        }
        let super_ctx = self.super_call_ctx(t.constr, ctx);
        let parents = self.parent_types(cls, &t.parents, super_ctx);
        self.check_parents(cls, rhs, &parents);
        if matches!(*self.ast.tree(rhs), Tree::Template { derived: Some(n), .. } if n > 0) {
            self.unsupported(rhs, "derives clause");
        }
        self.u.set_parents(cls, parents);
        if let Some(s) = t.self_
            && !self.is_module_self(s)
        {
            self.complete_self(cls, s, cctx);
        }
        self.finalize_value_class(cls);
        self.check_member_classes(cls, rhs);
        self.check_enum_case_refs(&t.body);
        self.check_module_owner(cls, rhs);
        let no_inits = self.has_no_inits(&t.parents, &t.body);
        self.syms.no_inits.insert(cls, no_inits);
        if !no_inits && !self.u.symbol(cls).flags.has(tasty::tags::TRAIT) {
            self.u.set_inits(cls);
        }
        self.set_stable_constructor(cls, t.constr);
        self.process_exports(cls, &t.body, cctx);
    }

    /// Whether enum class `c` declares or inherits a concrete `ordinal` other than `Enum`'s.
    fn defines_enum_ordinal(&mut self, c: SymbolId) -> bool {
        let flags = self.u.symbol(c).flags;
        if !flags.has(tasty::tags::ENUM) || flags.has(tasty::tags::CASE) {
            return false;
        }
        self.class_members(c, "ordinal").into_iter().any(|m| {
            let owner = self.u.symbol(m).owner.map(|o| self.u.full_name(o));
            !self.u.symbol(m).flags.has(tasty::tags::SYNTHETIC)
                && owner.as_deref() != Some("scala.reflect.Enum")
                && self.is_concrete(m)
        })
    }

    pub fn typed_class_def(&mut self, id: NodeId, ctx: Ctx) -> TNodeId {
        let Tree::TypeDef { name, rhs, .. } = *self.ast.tree(id) else { unreachable!() };
        let Tree::Template { .. } = self.ast.tree(rhs) else {
            return self.unsupported(id, "type definition");
        };
        let cls = self.syms.by_tree[&id];
        self.complete(cls);
        self.ensure_self(cls);
        if let Err(what) = self.check_parent_selves(cls) {
            return self.unsupported(id, what);
        }
        if self.defines_enum_ordinal(cls) {
            return self.unsupported(id, "enum class with its own ordinal");
        }
        let ctor = self.syms.primary_ctors.get(&cls).copied();
        if self.u.symbol(cls).flags.has(tasty::tags::CASE)
            && let Some(c) = ctor
        {
            self.complete(c);
            let info = self.u.info(c);
            let first = self.u.param_clauses(info).into_iter().next();
            let elems = first.map(|(_, ps)| ps).unwrap_or_default();
            if elems.iter().any(|&t| self.is_context_function_type(t)) {
                return self.unsupported(id, "case class element of a context function type");
            }
        }
        let t = self.template(rhs);
        let cctx = self.class_ctx(cls, ctx);
        let constr = self.typed(t.constr, Pt::Any, cctx);
        let name_span = Span::at(self.ast.span(id).point());
        let parents = self.parent_trees(cls, &t, ctx, name_span);
        self.check_trait_inits(cls, &parents, name_span);
        self.check_parent_prefixes(cls, &parents);
        let self_ = t.self_.map(|s| match self.is_module_self(s) {
            true => self.typed(s, Pt::Any, ctx),
            false => self.typed_self(s, cctx),
        });
        let (body, dummy) = self.typed_template_stats(cls, &t.body, cctx);
        let body = self.with_inline_members(cls, body, cctx);
        self.check_derived_value_class(id, cls, &parents, &t.body);
        self.check_overrides(cls, self.ast.span(id));
        self.check_base_types(cls, name_span);
        self.check_parameterized_traits(cls, name_span);
        let parents = self.tpd.list(&parents);
        let body = self.tpd.list(&body);
        let template = T::Template { constr, parents, self_, body };
        let template = self.tpd.node(template, self.ast.span(rhs), None, Some(dummy.unwrap_or(cls)));
        let span = self.ast.span(id);
        self.tpd.node(T::TypeDef { name, rhs: template }, span, None, Some(cls))
    }

    /// The statements of class `cls`'s body and the class's local dummy, which owns the
    /// expressions, if it has any.
    fn typed_template_stats(
        &mut self,
        cls: SymbolId,
        stats: &[NodeId],
        mut ctx: Ctx,
    ) -> (Vec<TNodeId>, Option<SymbolId>) {
        let mut dummy = None;
        let mut out = Vec::new();
        for &s in stats {
            let stat_ctx = match self.ast.tree(s) {
                Tree::Empty => continue,
                Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => ctx,
                Tree::Export { .. } => {
                    out.extend(self.typed_export(s, ctx));
                    continue;
                }
                _ => {
                    let name = format!("<local {}>", self.u.symbol(cls).name);
                    let d = *dummy.get_or_insert_with(|| {
                        self.syms.define(self.u, &name, Kind::Val, cls, syntax::ast::flags::EMPTY)
                    });
                    ctx.with_owner(d)
                }
            };
            out.push(self.typed_stat(s, stat_ctx));
            ctx = self.after_stat(s, ctx);
        }
        (out, dummy)
    }

    /// Whether script class `c` may have members its declarations lack: one its self type
    /// declares, or one an export clause in it defines, or would if it were not refused.
    pub fn has_undeclared_members(&self, c: SymbolId) -> bool {
        let Some((&tree, _)) = self.syms.by_tree.iter().find(|&(_, &s)| s == c) else { return false };
        let Tree::TypeDef { rhs, .. } = *self.ast.tree(tree) else { return false };
        let Tree::Template { self_, .. } = *self.ast.tree(rhs) else { return false };
        self_.is_some_and(|s| !self.is_module_self(s))
            || self.ast.exists_subtree(rhs, &|ast, n| matches!(ast.tree(n), Tree::Export { .. }))
    }

    /// Whether self definition `s` is the `_: O.type` desugaring gives an object's class.
    fn is_module_self(&self, s: NodeId) -> bool {
        matches!(*self.ast.tree(s), Tree::ValDef { name, tpt, .. }
            if self.text(name.name) == "_" && matches!(self.ast.tree(tpt), Tree::SingletonTypeTree { .. }))
    }
}
