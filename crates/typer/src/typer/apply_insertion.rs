// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! `apply` inserted on an applied value (dotty's `tryInsertApplyOrImplicit`), an extension
//! method `apply` if the member does not apply (`tryWithImplicitOnQualifier`).

use super::implicits::{Goal, QualifierView, Viewed};
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName, Tree};
use tasty::{Type, nme};

impl Typer<'_> {
    /// `f`, or `f.apply` if `f` is not a method, the alternative chosen by `pt`, arguments or
    /// type arguments; a context function value applied to its implicit arguments first unless
    /// the arguments are `using` ones, a method with an implicit clause before type arguments
    /// (dotty's `adaptToArgs`).
    pub fn with_apply(&mut self, f: TNodeId, pt: Pt, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let tpe = self.tpd.tpe(f);
        let info = self.method_info(tpe).unwrap_or(tpe);
        if let Some(result) = self.u.poly(info).map(|p| p.result)
            && !matches!(self.u.ty(result), Type::Method { .. } | Type::Poly { .. })
            && matches!(pt, Pt::Fun(_))
        {
            let applied = self.with_type_vars(f)?;
            return self.with_apply(applied, pt, ctx);
        }
        if matches!(pt, Pt::PolyFun(_)) && self.u.method(info).is_some_and(|m| m.implicit) {
            let applied = self.implicit_clause_applied(f, ctx)?;
            return if applied == f { Ok(f) } else { self.with_apply(applied, pt, ctx) };
        }
        if matches!(self.u.ty(info), Type::Method { .. } | Type::Poly { .. } | Type::Error) {
            return Ok(f);
        }
        if let Pt::Fun(p) = pt
            && !self.protos[p].using
            && let Some(applied) = self.context_function_applied(f, Pt::Any, ctx)
        {
            return self.with_apply(applied?, pt, ctx);
        }
        let tpe = self.widen_if_unstable(tpe);
        if let Some(apply) = self.poly_apply(f, tpe, self.tpd.span(f).to_synthetic()) {
            return Ok(apply);
        }
        let Some(found) = self.members_found(tpe, nme::APPLY, false) else {
            let proxy = match pt {
                Pt::Fun(_) | Pt::PolyFun(_) => self.proxy_companion(f, pt, ctx),
                _ => None,
            };
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
        if self.extension_visible(nme::APPLY, ctx)
            && let Some(QualifierView::Extension) =
                self.qualifier_view(f, (tpe, &found.syms), nme::APPLY, pt, ctx)
            && let Some(apply) = self.extension_apply(f, pt, ctx)
        {
            return Ok(apply);
        }
        let sym = match self.choose(&found.syms, Some(tpe), pt) {
            Err(_) if self.unusable_for_inference(pt) => return Ok(self.error_tree(self.tpd.span(f))),
            chosen => chosen.map_err(|e| e.what)?,
        };
        self.check_reference(sym, Some(tpe), ctx)?;
        let name = syntax::ast::TName { name: self.name(nme::APPLY), is_type: false };
        let apply_tpe = self.ref_type(Some(tpe), sym);
        let span = self.tpd.span(f).to_synthetic();
        Ok(self.tpd.node(T::Select { qualifier: f, name }, span, Some(apply_tpe), Some(sym)))
    }

    /// The span of the tree `f` adapts by applying context functions to implicit arguments, the
    /// span of `f` itself if it is no such adaptation.
    pub fn adapted_span(&self, f: TNodeId) -> Span {
        let context_apply = |t: TNodeId| {
            let owner = self.tpd.sym(t).and_then(|m| self.u.symbol(m).owner);
            owner.is_some_and(|c| self.u.full_name(c).starts_with("scala.ContextFunction"))
        };
        let mut t = f;
        while self.tpd.span(t).is_synthetic() {
            t = match *self.tpd.tree(t) {
                T::Apply { fun, .. } if context_apply(fun) => fun,
                T::Select { qualifier, .. } if context_apply(t) => qualifier,
                _ => return self.tpd.span(f),
            };
        }
        self.tpd.span(t)
    }

    /// `f.apply` as a call of an extension method `apply` in scope or in the implicit scope of
    /// `f`'s type, or of the `apply` of `f` converted by an implicit view, if there is one.
    fn extension_apply(&mut self, f: TNodeId, pt: Pt, ctx: Ctx) -> Option<TNodeId> {
        let name = TName { name: self.name(nme::APPLY), is_type: false };
        let span = self.tpd.span(f);
        let qualifier = self.spliced(f);
        let id = self.ast.node(Tree::Select { qualifier, name });
        self.ast.with_span(id, Span::synthetic(span.start(), span.end()));
        if self.extension_visible(nme::APPLY, ctx) {
            return Some(self.extension_select(id, f, name, pt, ctx));
        }
        let unsupported = self.unsupported.clone();
        match self.viewed(f, Goal::Member(nme::APPLY), ctx) {
            Ok(Some(Viewed::Extension(c))) => Some(self.implicit_extension_select(id, f, c, name, pt, ctx)),
            Ok(Some(Viewed::Converted(converted))) => self.with_apply(converted, pt, ctx).ok(),
            _ => {
                self.unsupported = unsupported;
                None
            }
        }
    }
}
