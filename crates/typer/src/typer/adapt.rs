//! Fitting a typed tree to what its context expects (dotty's `adapt`), as far as the subset
//! goes: a tree that fits as it is.

use super::{Pt, Typer};
use crate::tpd::TNodeId;
use tasty::Type;

impl Typer<'_> {
    /// Checks that `tree` fits `pt` as it is: a method is applied, a value conforms; a tree that
    /// needs dotty's `adapt` to fit is outside the subset.
    pub fn adapt(&mut self, tree: TNodeId, pt: Pt) {
        let Some(tpe) = self.tpd.get(tree).tpe else { return };
        if matches!(self.u.types().get(tpe), Type::Error) || matches!(pt, Pt::Args(_)) {
            return;
        }
        let span = self.tpd.span(tree);
        let method = self.method_info(tpe).map(|i| self.u.types().get(i).clone());
        if matches!(method, Some(Type::Method { .. } | Type::Poly { .. })) {
            return self.unsupported_at(span, "method value");
        }
        if let Pt::Type(pt) = pt
            && !self.u.is_subtype(tpe, pt)
        {
            self.unsupported_at(span, "adaptation");
        }
    }
}
