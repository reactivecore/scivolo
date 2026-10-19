// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Contexts.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The parent trees of a class when it is typed (dotty's `parentTrees`, `typedParent`,
//! `ensureConstrCall`, `superCallContext`).

use super::classes::Template;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::SymbolId;

impl Typer<'_> {
    /// The parent trees of class `cls`: those of `parents` typed in `ctx` without the duplicates
    /// desugaring added, and calls for the parent types the completer added, at the class name
    /// `name_span` (dotty's `parentTrees`).
    pub fn parent_trees(&mut self, cls: SymbolId, t: &Template, ctx: Ctx, name_span: Span) -> Vec<TNodeId> {
        let (parents, super_ctx) = (&t.parents, self.super_call_ctx(t.constr, ctx));
        let mut trees: Vec<TNodeId> = Vec::new();
        let mut seen = Vec::new();
        for &p in parents {
            let tree = self.typed_parent(cls, p, super_ctx);
            let psym = self.parent_class(tree);
            if psym.is_some() && seen.contains(&psym) && !self.ast.span(p).is_source_derived() {
                continue;
            }
            seen.push(psym);
            trees.push(tree);
        }
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
                out.push(self.ensure_constr_call(cls, tpt, super_ctx));
            }
        }
        out.extend_from_slice(rest);
        out
    }

    /// The class a parent tree extends: a constructor call's class, or its type's.
    fn parent_class(&mut self, tree: TNodeId) -> Option<SymbolId> {
        match self.tpd.sym(self.tpd.strip_apply(tree)) {
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
    pub fn super_call_ctx(&mut self, constr: NodeId, ctx: Ctx) -> Ctx {
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

    /// A parent: a constructor call, or a type made one if it is a class's parent class or a
    /// trait taking parameters.
    fn typed_parent(&mut self, cls: SymbolId, parent: NodeId, ctx: Ctx) -> TNodeId {
        if matches!(self.ast.tree(parent), Tree::Apply { .. }) {
            let call = self.typed(parent, Pt::Any, ctx);
            let called = self.called_class(call);
            self.check_parent_call(cls, call, called);
            return call;
        }
        let tpt = self.typed_type(parent, ctx);
        self.ensure_constr_call(cls, tpt, ctx)
    }

    /// `new P()` for parent type tree `parent` of class `cls`, typed in `ctx`, if `cls` is not a
    /// trait and `P` is a class, or a trait taking parameters that the superclass of `cls` does
    /// not extend (dotty's `ensureConstrCall`).
    fn ensure_constr_call(&mut self, cls: SymbolId, parent: TNodeId, ctx: Ctx) -> TNodeId {
        let tpe = self.tpd.tpe(parent);
        let Some(psym) = self.u.class_symbol(tpe) else { return parent };
        let called_trait = |t: &mut Self| t.trait_takes_params(psym) && !t.super_derives_from(cls, psym);
        if self.u.is_trait(cls) || self.u.is_trait(psym) && !called_trait(self) {
            return parent;
        }
        let span = self.tpd.span(parent);
        let synthetic = span.to_synthetic();
        let splice = self.ast.node(Tree::TypeTree);
        self.ast.with_span(splice, span);
        self.splices.insert(splice, parent);
        let new = self.ast.node(Tree::New { tpt: splice });
        self.ast.with_span(new, synthetic);
        let init = TName { name: self.name("<init>"), is_type: false };
        let select = self.ast.node(Tree::Select { qualifier: new, name: init });
        self.ast.with_span(select, synthetic);
        let args = self.ast.list(&[]);
        let apply = self.ast.node(Tree::Apply { fun: select, args });
        self.ast.with_span(apply, synthetic);
        let call = self.typed(apply, Pt::Any, ctx);
        self.type_args_at(call, span.start_pos());
        call
    }

    /// Moves the inferred type arguments of constructor call `call` to `at`.
    fn type_args_at(&mut self, call: TNodeId, at: Span) {
        let T::TypeApply { args, .. } = *self.tpd.tree(self.tpd.strip_apply(call)) else { return };
        for a in self.tpd.items(args).to_vec() {
            self.tpd.get_mut(a).span = at;
        }
    }
}
