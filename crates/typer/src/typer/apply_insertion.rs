//! `apply` inserted on an applied value (dotty's `tryInsertApplyOrImplicit`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use tasty::Type;

impl Typer<'_> {
    /// `f`, or `f.apply` if `f` is not a method, the alternative chosen by `pt`, arguments or
    /// type arguments.
    pub fn with_apply(&mut self, f: TNodeId, pt: Pt, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let tpe = self.tpd.tpe(f);
        let info = self.method_info(tpe).unwrap_or(tpe);
        if let Type::Poly { result, .. } = *self.u.types().get(info)
            && !matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. })
            && matches!(pt, Pt::Fun(_))
        {
            let applied = self.with_type_vars(f)?;
            return self.with_apply(applied, pt, ctx);
        }
        if matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. } | Type::Error) {
            return Ok(f);
        }
        let tpe = self.widen_if_unstable(tpe);
        let Some(found) = self.members_found(tpe, "apply", false) else {
            let proxy = if matches!(pt, Pt::Fun(_)) { self.proxy_companion(f, pt, ctx) } else { None };
            if let Some(proxy) = proxy {
                return Ok(proxy);
            }
            let dynamic = if let Pt::Fun(_) | Pt::PolyFun(_) = pt {
                self.dynamic_apply_insertion(f, ctx)?
            } else {
                None
            };
            return dynamic.ok_or("application of a non-method");
        };
        let sym = match self.choose(&found.syms, Some(tpe), pt) {
            Err(_) if self.unusable_for_inference(pt) => return Ok(self.error_tree(self.tpd.span(f))),
            chosen => chosen?,
        };
        self.check_reference(sym, Some(tpe), ctx)?;
        let name = syntax::ast::TName { name: self.name("apply"), is_type: false };
        let apply_tpe = self.ref_type(Some(tpe), sym);
        let span = self.tpd.span(f).to_synthetic();
        Ok(self.tpd.node(T::Select { qualifier: f, name }, span, Some(apply_tpe), Some(sym)))
    }
}
