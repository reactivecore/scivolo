// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Function types with erased parameters `(erased x: A, y: B) => R`, also those erased by their
//! class (`CanThrow[E] ?=> R`): `PolyFunction { def apply(erased x: A, y: B): R }` (dotty's
//! `typedDependent`, `PolyFunctionOf`).

use super::Typer;
use super::dependent_function_types::ApplyParts;
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// Function type `id` of arguments `args` (types or parameter definitions) and result `body`,
    /// those marked in `erased` erased, a context function type if `contextual`; `None` if no
    /// parameter is erased, by its modifier or its class.
    pub fn typed_erased_function_type(
        &mut self,
        id: NodeId,
        (args, body): (&[NodeId], NodeId),
        (erased, contextual): (&[bool], bool),
        ctx: Ctx,
    ) -> Option<TNodeId> {
        let refinement = self.refinement_class(ctx);
        let owner = self.syms.define(self.u, "apply", Kind::Def, refinement, f::METHOD);
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        let lctx = Ctx { level, ..ctx };
        let (mut syms, mut trees, mut any_erased) = (Vec::new(), Vec::new(), false);
        for (i, &a) in args.iter().enumerate() {
            let (name, tpt, synthetic) = match *self.ast.tree(a) {
                Tree::ValDef { name, tpt, .. } => (self.text(name.name).to_owned(), tpt, false),
                _ => (format!("x${}", i + 1), a, true),
            };
            if matches!(self.ast.tree(tpt), Tree::ByNameTypeTree { .. }) {
                return Some(self.unsupported(a, "erased function type with a by-name parameter"));
            }
            let typed = self.typed_type(tpt, lctx);
            if self.is_error(typed) {
                return Some(typed);
            }
            let tpe = self.tpd.tpe(typed);
            if self.refers_to(tpe, &syms) {
                return Some(self.unsupported(a, "function type with inter-parameter dependencies"));
            }
            let is_erased = erased.get(i).copied().unwrap_or(false) || self.derives_erased_class(tpe);
            any_erased |= is_erased;
            let mut flags = if contextual { f::TERM_PARAM.or(f::GIVEN) } else { f::TERM_PARAM };
            if is_erased {
                flags = flags.or(f::ERASED);
            }
            if synthetic {
                flags = flags.or(f::SYNTHETIC);
            }
            let s = self.syms.define(self.u, &name, Kind::Param, owner, flags.to_term_flags());
            self.u.set_info(s, tpe);
            self.levels.enter(level, &name, s);
            syms.push(s);
            let span = if synthetic { self.ast.span(a).to_synthetic() } else { self.ast.span(a) };
            trees.push(self.build().val_def(s, typed, None, span));
        }
        if !any_erased {
            return None;
        }
        let result_tree = self.typed_type(body, lctx);
        if self.is_error(result_tree) {
            return Some(result_tree);
        }
        let result = self.tpd.tpe(result_tree);
        let tpe = self.erased_function_of(&syms, result, contextual)?;
        let Type::Refined(core, _, info) = *self.u.types().get(tpe) else { unreachable!("a refined type") };
        self.u.set_info(owner, info);
        let span = self.ast.span(id);
        let apply = self.build().def_def(owner, &[&trees], result_tree, None, span);
        let tpt = self.type_tree(core, span.start_pos(), false);
        let refinements = self.tpd.list(&[apply]);
        Some(self.tpd.node(T::RefinedTypeTree { tpt, refinements }, span.to_synthetic(), Some(tpe), None))
    }

    /// The parameter types and result of context function type `t`,
    /// `PolyFunction { def apply(using params): result }`, and whether each parameter is erased.
    pub fn erased_context_function(&mut self, t: TypeId) -> Option<(Vec<TypeId>, TypeId, Vec<bool>)> {
        let (params, result) = self.erased_context_names(t)?;
        let erased = params.iter().map(|&(_, p)| self.u.is_erased_param(p)).collect();
        Some((params.into_iter().map(|(_, p)| p).collect(), result, erased))
    }

    /// The parameters and result of the `apply` of context function type `t`,
    /// `PolyFunction { def apply(using params): result }`.
    pub fn erased_context_names(&mut self, t: TypeId) -> Option<(Vec<(String, TypeId)>, TypeId)> {
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        let info = self.u.poly_function_apply(t)?;
        let m = self.u.method(info).filter(|m| m.contextual)?;
        Some((m.params.to_vec(), m.result))
    }

    /// The parameter types and result of the `apply` of
    /// `PolyFunction { def apply(params): result }` type `t` of `n` parameters, contextual if
    /// `contextual`, and the binder and names of its parameters.
    pub fn erased_function_parts(&mut self, t: TypeId, n: usize, contextual: bool) -> Option<ApplyParts> {
        let t = self.zonk(t);
        let t = self.u.dealias(t);
        let info = self.u.poly_function_apply(t)?;
        let m = self.u.method(info).filter(|m| m.params.len() == n && m.contextual == contextual)?;
        let (names, types) = m.params.iter().cloned().unzip();
        Some(((types, m.result), (m.binder, names)))
    }

    /// Parameter type `t` without its `@ErasedParam` (dotty's `stripAnnots` of an inferred
    /// parameter type).
    pub fn without_erased_param(&mut self, t: TypeId) -> TypeId {
        let Type::Annotated(inner, _) = *self.u.types().get(t) else { return t };
        if self.u.is_erased_param(t) { inner } else { t }
    }

    /// `PolyFunction { def apply(params): result }`, the parameters flagged erased annotated
    /// `@ErasedParam` and referred to as parameters of the method (dotty's `toFunctionType`).
    pub fn erased_function_of(
        &mut self,
        params: &[tasty::SymbolId],
        result: TypeId,
        contextual: bool,
    ) -> Option<TypeId> {
        let class = self.resolve_full_name("scala.PolyFunction", true)?;
        let core = crate::defn::type_ref(self.u, class);
        let binder = self.syms.fresh_binder();
        let names =
            params.iter().map(|&p| (self.u.symbol(p).name.clone(), self.method_param_type(p))).collect();
        let result = self.u.map_type(result, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => {
                let i = params.iter().position(|&p| p == s)?;
                Some(u.intern_type(Type::Param(binder, i as u32)))
            }
            _ => None,
        });
        let method = Type::Method { binder, params: names, result, implicit: contextual, contextual };
        let method = self.u.intern_type(method);
        Some(self.u.intern_type(Type::Refined(core, "apply".into(), method)))
    }
}
