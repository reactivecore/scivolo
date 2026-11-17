// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/NamerOps.scala
//! The refinements a constructor's tracked parameters add to the type it constructs.

use super::Typer;
use tasty::tags::TRACKED;
use tasty::{Binder, ParamList, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Refines `class`, the type a constructor of class `owner` constructs, by `{ val p: p.type }`
    /// for each tracked parameter `p` of the clause `params` of the method type `binder` (dotty's
    /// `addParamRefinements`).
    pub fn add_param_refinements(
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
            let tracked = decls.iter().any(|&d| self.u.symbol(d).flags.has(TRACKED));
            if !tracked {
                return class;
            }
            let param = self.u.intern_type(Type::Param(binder, i as u32));
            self.u.intern_type(Type::Refined(class, name, param))
        })
    }

    /// `t` without the refinements around its class type.
    pub fn without_refinements(&self, mut t: TypeId) -> TypeId {
        while let Type::Refined(parent, ..) = *self.u.ty(t) {
            t = parent;
        }
        t
    }
}
