// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Selections `qual.name` (dotty's `typedSelect`).

use super::gadt_cast::GADT_HEALED_VIEW;
use super::implicits::{Goal, QualifierView, Viewed};
use super::opaque_paths::OPAQUE_THROUGH_OBJECT;
use super::source_splices::STAGING_REFERENCE;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, TName, Tree as S};
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    pub fn typed_select(&mut self, id: NodeId, qualifier: NodeId, name: TName, pt: Pt, ctx: Ctx) -> TNodeId {
        let path = matches!(self.ast.tree(qualifier), S::Ident { .. } | S::Select { .. });
        let text = self.text(name.name).to_owned();
        let mode = if path { Mode::QUALIFIER } else { Mode::NONE };
        let mode = if text == "apply" && !name.is_type { mode.with(Mode::APPLY_QUALIFIER) } else { mode };
        let mut qual = self.typed(qualifier, Pt::Any, ctx.with_mode(mode));
        if self.is_probe_target(id) {
            return self.probe_selection(id, qual, ctx);
        }
        if self.is_error(qual) {
            return self.error_tree(self.ast.span(id));
        }
        let (mut converted, mut widened, mut tupled, mut defined, mut healed) =
            (false, false, false, false, false);
        let (qual_tpe, syms) = loop {
            let qual_tpe = self.tpd.tpe(qual);
            let qual_tpe = self.instantiated_selected(qual_tpe);
            let qual_tpe = self.widen_if_unstable(qual_tpe);
            let syms: Vec<SymbolId> = self
                .type_members_in(qual_tpe, &text, ctx)
                .into_iter()
                .filter(|&s| self.u.is_type_symbol(s) == name.is_type)
                .collect();
            if syms.is_empty() && self.needs_joint_member(qual_tpe, &text) {
                return self.unsupported(id, "intersection member of an intersected type");
            }
            if !syms.is_empty()
                && !converted
                && !name.is_type
                && let Some(view) = self.qualifier_view(qual, (qual_tpe, &syms), &text, pt, ctx)
            {
                match view {
                    QualifierView::Converted(q) => (qual, converted) = (q, true),
                    QualifierView::Extension => {
                        let call = self.extension_select(id, qual, name, pt, ctx);
                        let span = self.ast.span(id);
                        self.tpd.get_mut(call).span = span.to_synthetic();
                        return call;
                    }
                }
                continue;
            }
            if !syms.is_empty()
                && !converted
                && !name.is_type
                && !self.has_selectable(&syms, qual_tpe, &text, ctx)
                && let Ok(Some(Viewed::Converted(q))) = self.viewed(qual, Goal::Member(&text), ctx)
            {
                (qual, converted) = (q, true);
                continue;
            }
            if !syms.is_empty() || converted || name.is_type {
                break (qual_tpe, syms);
            }
            if let Some(cast) = self.java_null_stripped(qual) {
                qual = cast;
                continue;
            }
            if self.opaque_member_through_object(qual_tpe, &text, ctx) {
                return self.unsupported(id, OPAQUE_THROUGH_OBJECT);
            }
            if let Some(apply) = self.poly_function_select(id, qual, qual_tpe, name) {
                return apply;
            }
            if let Some(structural) = self.structural_select(id, (qual, qual_tpe), name, ctx) {
                return structural;
            }
            if matches!(pt, Pt::Fun(_) | Pt::PolyFun(_))
                && let Some(proxy) = self.proxy_select(id, (qual, qual_tpe), name, pt, ctx)
            {
                return proxy;
            }
            if let Some(element) = self.named_tuple_select(id, qual, name, pt, ctx) {
                return element;
            }
            if !tupled && let Some(q) = self.small_generic_tuple(qual) {
                (qual, tupled) = (q, true);
                continue;
            }
            if let Some(case) = self.widened_enum_case(qual) {
                qual = case;
                widened = true;
                continue;
            }
            if self.extension_visible(&text, ctx)
                && !(self.extensions_reject(qual, &text, ctx) && self.implicit_member_found(qual, &text, ctx))
            {
                return self.extension_select(id, qual, name, pt, ctx);
            }
            match self.viewed(qual, Goal::of_select(&text, pt), ctx) {
                Ok(None) if !healed && self.gadt_narrowing() => {
                    (qual, healed) = (self.gadt_healed(qual, ctx).unwrap_or(qual), true);
                    continue;
                }
                Ok(Some(Viewed::Converted(_))) if healed => return self.unsupported(id, GADT_HEALED_VIEW),
                Ok(Some(Viewed::Converted(q))) => qual = q,
                Ok(Some(Viewed::Extension(c))) => {
                    return self.implicit_extension_select(id, qual, c, name, pt, ctx);
                }
                Ok(None) if !defined && self.defined_fail_bottom(qual_tpe).is_some() => {
                    defined = true;
                    continue;
                }
                Ok(None) if !widened => {
                    let dynamic = self.try_dynamic(id, qual, name, pt, ctx);
                    let dynamic = dynamic.or_else(|| self.selectable_field(id, (qual, qual_tpe), name, ctx));
                    return dynamic.unwrap_or_else(|| self.member_not_found(id, qual, name, ctx));
                }
                Ok(None) => return self.unsupported(id, "member not found"),
                Err(what) => return self.unsupported(id, what),
            }
            converted = true;
        };
        if let Some(projection) = self.refined_projection(id, qual, qual_tpe, name) {
            return projection;
        }
        if let Some(refined) = self.refined_member(id, (qual, qual_tpe), name, (&syms, widened), ctx) {
            return refined;
        }
        if syms.is_empty() {
            return self.unsupported(id, "member not found");
        }
        let syms = match converted {
            true => syms,
            false => self.accessible_or_inherited(syms, qual_tpe, &text, ctx),
        };
        let sym = match self.choose(&syms, Some(qual_tpe), pt) {
            Ok(sym) => self.jdk_inherited(sym),
            Err(what) => return self.unchosen(id, what, pt),
        };
        if !converted && !self.is_accessible_from(sym, Some(qual_tpe), ctx) {
            if self.extension_visible(&text, ctx) {
                return self.extension_select(id, qual, name, pt, ctx);
            }
            if let Some(dynamic) = self.try_dynamic_inaccessible(id, qual, name, pt, ctx) {
                return dynamic;
            }
        }
        if let Err(what) = self
            .check_reference(sym, Some(qual_tpe), ctx)
            .and_then(|()| self.check_erased_reference(sym, pt, ctx))
        {
            return self.unsupported(id, what);
        }
        if self.jdk_redeclares(sym, qual_tpe) {
            return self.unsupported(id, "Object member a JDK interface redeclares");
        }
        if let Err(what) = self.check_value(sym, name, pt, ctx) {
            let Some(object) = self.package_object_value(sym, ctx) else { return self.unsupported(id, what) };
            let mode = ctx.mode.with(Mode::QUALIFIER);
            let path = self.typed_select(id, qualifier, name, Pt::Any, ctx.with_mode(mode));
            let span = self.ast.span(id);
            return self.build().select(path, object, span);
        }
        let new = matches!(self.tpd.tree(qual), T::New { .. });
        if let Err(what) = self.check_protected_path(sym, qual_tpe, new, ctx) {
            return self.unsupported(id, what);
        }
        let this = matches!(self.tpd.tree(qual), T::This { .. });
        let projection = !this && self.tpd.sym(qual).is_some_and(|q| self.u.is_type_symbol(q))
            || matches!(self.tpd.tree(qual), T::AppliedTypeTree { .. });
        if projection && self.u.symbol(sym).kind == Kind::Type {
            self.complete(sym);
            let info = self.u.info(sym);
            if let Type::Alias(alias) = *self.u.types().get(info) {
                return self.alias_projection(id, (qual, name), sym, alias);
            }
        }
        if text == "deferred" && self.u.full_name(sym).starts_with("scala.compiletime.") {
            return self.unsupported(id, "deferred given");
        }
        if self.is_staging_method(sym) && !matches!(self.ast.tree(qualifier), S::TypedSplice { .. }) {
            return self.unsupported(id, STAGING_REFERENCE);
        }
        let tpe = self.ref_type(Some(qual_tpe), sym);
        if name.is_type && self.is_opaque_through_object(tpe) {
            return self.unsupported(id, OPAQUE_THROUGH_OBJECT);
        }
        let span = self.ast.span(id);
        let select = self.tpd.node(T::Select { qualifier: qual, name }, span, Some(tpe), Some(sym));
        self.fold_select(select, &text);
        select
    }
}
