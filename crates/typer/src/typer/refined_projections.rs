// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Projections of refined type trees reduced to the alias a refinement gives.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, TName};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Projection `qual#name` of refined type tree `qual`, of type `pre`, whose refinement makes
    /// `name` an alias: that alias (dotty's `reduceProjection`); `None` otherwise.
    pub fn refined_projection(
        &mut self,
        id: NodeId,
        qual: TNodeId,
        pre: TypeId,
        name: TName,
    ) -> Option<TNodeId> {
        if !name.is_type || !matches!(self.tpd.tree(qual), T::RefinedTypeTree { .. }) {
            return None;
        }
        let text = self.text(name.name).to_owned();
        let alias = self.lookup_refined(pre, &text)?;
        let sym = match *self.u.types().get(alias) {
            Type::Sym(_, s) => Some(s),
            _ => None,
        };
        let span = self.ast.span(id);
        Some(self.tpd.node(T::Select { qualifier: qual, name }, span, Some(alias), sym))
    }

    /// The alias the outermost alias refinement of `name` in `t` gives it, through annotations
    /// and type aliases (dotty's `lookupRefined`).
    fn lookup_refined(&mut self, t: TypeId, name: &str) -> Option<TypeId> {
        match self.u.types().get(t).clone() {
            Type::Refined(p, n, info) => match *self.u.types().get(info) {
                Type::Alias(alias) if n == name => Some(alias),
                _ => self.lookup_refined(p, name),
            },
            Type::Annotated(p, _) => self.lookup_refined(p, name),
            Type::TypeRef(..) | Type::Sym(..) if self.u.dealias(t) != t => {
                let d = self.u.dealias(t);
                self.lookup_refined(d, name)
            }
            _ => None,
        }
    }
}
