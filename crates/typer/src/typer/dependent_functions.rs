// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Dependent function type trees `(x: A) => B` (dotty's `typedDependent`) and the dependent
//! function types of closures.

use super::Typer;
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::{Binder, Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Function type `id` of the parameter definitions `params` and result `body`, a context
    /// function type if `contextual`, `class` its `FunctionN` or `ContextFunctionN`.
    pub fn typed_dependent_function_type(
        &mut self,
        id: NodeId,
        (params, body): (&[NodeId], NodeId),
        (class, contextual): (TypeId, bool),
        ctx: Ctx,
    ) -> TNodeId {
        let refinement = self.refinement_class(ctx);
        let owner = self.syms.define(self.u, "apply", Kind::Def, refinement, f::METHOD);
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        let lctx = Ctx { level, ..ctx };
        let (mut syms, mut trees, mut spans) = (Vec::new(), Vec::new(), Vec::new());
        for &p in params {
            let Tree::ValDef { name, tpt, .. } = *self.ast.tree(p) else {
                return self.unsupported(p, "function type parameter");
            };
            if matches!(self.ast.tree(tpt), Tree::ByNameTypeTree { .. }) {
                return self.unsupported(p, "dependent function type with a by-name parameter");
            }
            let typed = self.typed_type(tpt, lctx);
            if self.is_error(typed) {
                return typed;
            }
            let tpe = self.tpd.tpe(typed);
            spans.push(self.tpd.span(typed));
            if matches!(self.u.types().get(tpe), Type::Bounds(..)) {
                return self.unsupported(p, "function type parameter of a wildcard type");
            }
            if self.derives_erased_class(tpe) {
                return self.unsupported(p, "dependent function type with a parameter of an erased class");
            }
            if self.refers_to(tpe, &syms) {
                return self.unsupported(p, "function type with inter-parameter dependencies");
            }
            let text = self.text(name.name).to_owned();
            let flags = if contextual { f::TERM_PARAM.or(f::GIVEN) } else { f::TERM_PARAM };
            let s = self.syms.define(self.u, &text, Kind::Param, owner, flags.to_term_flags());
            self.u.set_info(s, tpe);
            self.levels.enter(level, &text, s);
            syms.push(s);
            let span = self.ast.span(p);
            trees.push(self.build().val_def(s, typed, None, span));
        }
        let result_tree = self.typed_type(body, lctx);
        if self.is_error(result_tree) {
            return result_tree;
        }
        let result = self.tpd.tpe(result_tree);
        let binder = self.syms.fresh_binder();
        let Some(tpe) = self.dependent_function_of((class, contextual), &syms, binder, result) else {
            return self.unsupported(id, "dependent function type whose result cannot be approximated");
        };
        let Type::Refined(core, _, info) = *self.u.types().get(tpe) else { unreachable!("a refined type") };
        self.u.set_info(owner, info);
        let span = self.ast.span(id);
        let apply = self.build().def_def(owner, &[&trees], result_tree, None, span);
        let Type::Applied(_, ref args) = *self.u.types().get(core) else { unreachable!("a function type") };
        let args = args.clone();
        spans.push(self.tpd.span(result_tree));
        let arg_trees: Vec<TNodeId> =
            args.iter().zip(spans).map(|(&a, s)| self.type_tree(a, s, false)).collect();
        let first = arg_trees.first().map_or(span, |&t| self.tpd.span(t));
        let tycon = self.type_tree(class, first.start_pos(), false);
        let args = self.tpd.list(&arg_trees);
        let core_span = first.union(self.tpd.span(result_tree)).to_synthetic();
        let tpt = self.tpd.node(T::AppliedTypeTree { tpt: tycon, args }, core_span, Some(core), None);
        let refinements = self.tpd.list(&[apply]);
        self.tpd.node(T::RefinedTypeTree { tpt, refinements }, span.to_synthetic(), Some(tpe), None)
    }

    /// The dependent function type of a closure of parameters `params` whose result type is the
    /// first of `checked`, if one of `checked` refers to them; `Err` if it cannot be approximated.
    pub fn closure_dependent_type(
        &mut self,
        function: (TypeId, bool),
        params: &[SymbolId],
        binder: Binder,
        checked: &[TypeId],
    ) -> Result<Option<TypeId>, &'static str> {
        let mut dependent = false;
        for &t in checked {
            let t = self.zonk(t);
            let t = self.widen_singletons(t, false);
            self.u.map_type(t, &mut |u, part| {
                dependent |= matches!(*u.types().get(part), Type::Sym(_, s) if params.contains(&s));
                None
            });
        }
        match dependent {
            true => self
                .dependent_function_of(function, params, binder, checked[0])
                .map(Some)
                .ok_or("dependent function type"),
            false => Ok(None),
        }
    }
}
