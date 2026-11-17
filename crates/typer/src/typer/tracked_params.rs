// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NamerOps.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The result type of a constructor, refined by its tracked parameters (dotty's
//! `effectiveResultType`), and the parent refinements they give.

use super::Typer;
use syntax::ast::NodeId;
use tasty::tags::TRACKED;
use tasty::{Binder, ParamList, SymbolId, Type, TypeId};

/// The parameter clauses of a method type, by binder.
pub type Clauses = Vec<(Binder, ParamList)>;

impl Typer<'_> {
    /// Determines method type `t` of a constructor of `owner` with its final result, if `Unit`,
    /// replaced by `class`, refined by the tracked parameters of the clauses `outer` and of `t`.
    pub fn with_final_result(
        &mut self,
        owner: SymbolId,
        t: TypeId,
        class: TypeId,
        outer: &mut Clauses,
    ) -> TypeId {
        let node = *self.u.ty(t);
        let result = match node {
            Type::Method { binder, params, result, implicit, contextual } => {
                outer.push((binder, params));
                let result = self.with_final_result(owner, result, class, outer);
                return self.u.intern_type(Type::Method { binder, params, result, implicit, contextual });
            }
            _ if self.u.class_symbol(t) == Some(self.defn.unit) => class,
            _ if self.u.class_symbol(t) == Some(owner) && self.without_refinements(t) == t => t,
            _ => return t,
        };
        outer.iter().fold(result, |r, &(binder, params)| self.add_param_refinements(owner, r, binder, params))
    }

    /// Refines `class`, the type a constructor of class `owner` constructs, by `{ val p: p.type }`
    /// for each tracked parameter `p` of the clause `params` of the method type `binder` (dotty's
    /// `addParamRefinements`).
    fn add_param_refinements(
        &mut self,
        owner: SymbolId,
        class: TypeId,
        binder: Binder,
        params: ParamList,
    ) -> TypeId {
        if !self.u.symbol(owner).flags.has(TRACKED) {
            return class;
        }
        let names: Vec<_> = self.u.params_of(params).iter().map(|&(n, _)| n).collect();
        names.into_iter().enumerate().fold(class, |class, (i, name)| {
            let decls = self.u.decl(owner, name).unwrap_or_default();
            if !decls.iter().any(|&d| self.u.symbol(d).flags.has(TRACKED)) {
                return class;
            }
            let param = self.u.intern_type(Type::Param(binder, i as u32));
            self.u.intern_type(Type::Refined(class, name, param))
        })
    }

    /// Refuses parent `p` of class `cls` whose constructor call, of type `tpe`, refines a tracked
    /// parameter `cls` does not declare beyond its declared type, as `"bar"` in
    /// `class Bar extends Foo("bar")` (dotty's `enterParentRefinementSyms`).
    pub fn check_parent_refinements(&mut self, cls: SymbolId, p: NodeId, tpe: TypeId) {
        let mut t = tpe;
        while let Type::Refined(parent, name, info) = *self.u.ty(t) {
            t = parent;
            if !self.u.decl(cls, name).unwrap_or_default().is_empty() {
                continue;
            }
            let Some(c) = self.u.class_symbol(parent) else { continue };
            let decls = self.u.decl(c, name).unwrap_or_default();
            let declared = decls.into_iter().find(|&d| self.u.symbol(d).flags.has(TRACKED));
            if declared.is_some_and(|d| {
                let declared = self.u.info(d);
                !self.u.is_subtype(declared, info)
            }) {
                self.unsupported(p, "parent refinement of a tracked parameter");
                return;
            }
        }
    }

    /// `t` without the refinements around its class type.
    pub fn without_refinements(&self, mut t: TypeId) -> TypeId {
        while let Type::Refined(parent, ..) = *self.u.ty(t) {
            t = parent;
        }
        t
    }
}
