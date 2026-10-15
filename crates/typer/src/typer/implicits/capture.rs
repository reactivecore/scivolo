//! Wildcard capture of a view's argument type (dotty's `Inferencing.captureWildcards`).

use super::super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `t` with each top-level wildcard argument `_ >: lo <: hi` replaced by the `CAP` member of
    /// a fresh skolem of type `scala.runtime.TypeBox[lo, hi]`; `None` if `t` has none.
    pub fn captured_wildcards(&mut self, t: TypeId) -> Option<TypeId> {
        let t = self.zonk(t);
        let t = self.u.widen(t);
        let Type::Applied(tycon, args) = self.u.types().get(t).clone() else { return None };
        if !args.iter().any(|&a| matches!(self.u.types().get(a), Type::Bounds(..))) {
            return None;
        }
        let type_box = self.resolve_full_name("scala.runtime.TypeBox", true)?;
        let cap = self.u.decl(type_box, "CAP").ok()?.first().copied()?;
        let mut captured = Vec::new();
        for a in args {
            let Type::Bounds(lo, hi) = *self.u.types().get(a) else {
                captured.push(a);
                continue;
            };
            let boxed = crate::defn::applied(self.u, type_box, vec![lo, hi]);
            let skolem = self.u.fresh_skolem(boxed);
            captured.push(self.u.intern_type(Type::Sym(Some(skolem), cap)));
        }
        Some(self.u.intern_type(Type::Applied(tycon, captured)))
    }

    /// `tree.$asInstanceOf$[to]`, at `tree`'s span made synthetic, `to` at its end.
    pub fn cast_to_captured(&mut self, tree: TNodeId, to: TypeId) -> Result<TNodeId, &'static str> {
        let span = self.tpd.span(tree).to_synthetic();
        let any = self.u.builtins().any;
        let found = self.u.decl(any, "$asInstanceOf$").ok().and_then(|d| d.first().copied());
        let fun = self.build().select(tree, found.ok_or("Any.$asInstanceOf$")?, span);
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, span.end_pos(), Some(to), None);
        Ok(self.build().type_apply(fun, &[tpt], to, span))
    }
}
