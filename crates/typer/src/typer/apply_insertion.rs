// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! `apply` inserted on an applied value (dotty's `tryInsertApplyOrImplicit`).

use super::implicits::conversions::{Goal, Viewed};
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName, Tree};
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
        if let Some(apply) = self.poly_apply(f, tpe, self.tpd.span(f).to_synthetic()) {
            return Ok(apply);
        }
        let Some(found) = self.members_found(tpe, "apply", false) else {
            let proxy = if matches!(pt, Pt::Fun(_)) { self.proxy_companion(f, pt, ctx) } else { None };
            if let Some(proxy) = proxy {
                return Ok(proxy);
            }
            if let Pt::Fun(_) = pt
                && let Some(apply) = self.extension_apply(f, pt, ctx)
            {
                return Ok(apply);
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

    /// `f.apply` as a call of an extension method `apply` in scope or in the implicit scope of
    /// `f`'s type, if there is one.
    fn extension_apply(&mut self, f: TNodeId, pt: Pt, ctx: Ctx) -> Option<TNodeId> {
        let name = TName { name: self.name("apply"), is_type: false };
        let span = self.tpd.span(f);
        let qualifier = self.spliced(f);
        let id = self.ast.node(Tree::Select { qualifier, name });
        self.ast.with_span(id, Span::synthetic(span.start(), span.end()));
        if self.extension_visible("apply", ctx) {
            return Some(self.extension_select(id, f, name, pt, ctx));
        }
        match self.viewed(f, Goal::Member("apply"), ctx) {
            Ok(Some(Viewed::Extension(c))) => Some(self.implicit_extension_select(id, f, c, name, pt, ctx)),
            _ => None,
        }
    }
}
