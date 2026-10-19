//! Selections `qual.name` (dotty's `typedSelect`).

use super::implicits::conversions::{Goal, Viewed};
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, TName};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_select(&mut self, id: NodeId, qualifier: NodeId, name: TName, pt: Pt, ctx: Ctx) -> TNodeId {
        let path = matches!(
            self.ast.tree(qualifier),
            syntax::ast::Tree::Ident { .. } | syntax::ast::Tree::Select { .. }
        );
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
        if text == "*:" && self.is_large_generic_tuple(qual) {
            return self.unsupported(id, "generic tuple of more than 22 elements");
        }
        if text == "apply" && self.is_eta_closure(qual) {
            return self.unsupported(id, "apply of a method reference");
        }
        let (mut converted, mut widened, mut tupled, mut defined) = (false, false, false, false);
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
                && let Some(q) = self.qualifier_view(qual, (qual_tpe, &syms), &text, pt, ctx)
            {
                (qual, converted) = (q, true);
                continue;
            }
            if !syms.is_empty() || converted || name.is_type {
                break (qual_tpe, syms);
            }
            if !defined && self.defined_fail_bottom(qual_tpe).is_some() {
                defined = true;
                continue;
            }
            if let Some(structural) = self.structural_select(id, (qual, qual_tpe), name, pt, ctx) {
                return structural;
            }
            if matches!(pt, Pt::Fun(_) | Pt::PolyFun)
                && let Some(proxy) = self.proxy_select(id, (qual, qual_tpe), name, pt, ctx)
            {
                return proxy;
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
            if self.extension_visible(&text, ctx) {
                return self.extension_select(id, qual, name, pt, ctx);
            }
            match self.viewed(qual, Goal::Member(&text), ctx) {
                Ok(Some(Viewed::Converted(q))) => qual = q,
                Ok(Some(Viewed::Extension(c))) => {
                    return self.implicit_extension_select(id, qual, c, name, pt, ctx);
                }
                Ok(None) if !widened => {
                    let dynamic = self.try_dynamic(id, qual, name, pt, ctx);
                    return dynamic.unwrap_or_else(|| self.member_not_found(id, qual, name, ctx));
                }
                Ok(None) => return self.unsupported(id, "member not found"),
                Err(what) => return self.unsupported(id, what),
            }
            converted = true;
        };
        if syms.is_empty() && name.is_type && !widened {
            let refined = self.refined_select(id, qual, qual_tpe, name);
            return refined.unwrap_or_else(|| self.member_not_found(id, qual, name, ctx));
        }
        if syms.is_empty() {
            return self.unsupported(id, "member not found");
        }
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
        if let Err(what) = self.check_reference(sym, Some(qual_tpe), ctx) {
            return self.unsupported(id, what);
        }
        if self.jdk_redeclares(sym, qual_tpe) {
            return self.unsupported(id, "Object member a JDK interface redeclares");
        }
        if let Err(what) = self.check_value(sym, name, matches!(pt, Pt::Fun(_)), ctx) {
            return self.unsupported(id, what);
        }
        let new = matches!(self.tpd.tree(qual), T::New { .. });
        if let Err(what) = self.check_protected_path(sym, qual_tpe, new, ctx) {
            return self.unsupported(id, what);
        }
        let projection = self.tpd.sym(qual).is_some_and(|q| self.u.is_type_symbol(q))
            || matches!(self.tpd.tree(qual), T::AppliedTypeTree { .. });
        if projection && self.u.symbol(sym).kind == Kind::Type {
            self.complete(sym);
            let info = self.u.info(sym);
            if matches!(self.u.types().get(info), Type::Alias(_)) {
                return self.unsupported(id, "projection of a type alias");
            }
        }
        if text == "deferred" && self.u.full_name(sym).starts_with("scala.compiletime.") {
            return self.unsupported(id, "deferred given");
        }
        let tpe = self.ref_type(Some(qual_tpe), sym);
        let span = self.ast.span(id);
        let select = self.tpd.node(T::Select { qualifier: qual, name }, span, Some(tpe), Some(sym));
        self.fold_select(select, &text);
        select
    }

    /// Whether member `sym` of `Any` or `Object`, selected from `qual`, is one a JDK interface
    /// the class of `qual` derives from declares.
    pub fn jdk_redeclares(&mut self, sym: SymbolId, qual: TypeId) -> bool {
        const REDECLARED: [(&str, &str); 22] = [
            ("java.lang.CharSequence", "toString"),
            ("java.util.Comparator", "equals"),
            ("java.util.Collection", "equals"),
            ("java.util.Collection", "hashCode"),
            ("java.util.Map", "equals"),
            ("java.util.Map", "hashCode"),
            ("java.util.Map$.Entry", "equals"),
            ("java.util.Map$.Entry", "hashCode"),
            ("java.lang.annotation.Annotation", "equals"),
            ("java.lang.annotation.Annotation", "hashCode"),
            ("java.lang.annotation.Annotation", "toString"),
            ("java.nio.file.Path", "equals"),
            ("java.nio.file.Path", "hashCode"),
            ("java.nio.file.Path", "toString"),
            ("java.time.chrono.ChronoLocalDate", "equals"),
            ("java.time.chrono.ChronoLocalDate", "hashCode"),
            ("java.time.chrono.ChronoLocalDate", "toString"),
            ("java.time.chrono.ChronoLocalDateTime", "equals"),
            ("java.time.chrono.ChronoLocalDateTime", "hashCode"),
            ("java.time.chrono.ChronoLocalDateTime", "toString"),
            ("java.util.Set", "equals"),
            ("java.util.List", "equals"),
        ];
        let b = self.u.builtins();
        if !self.u.symbol(sym).owner.is_some_and(|o| o == b.any || o == b.object) {
            return false;
        }
        let name = self.u.symbol(sym).name.clone();
        let widened = self.u.widen(qual);
        let Some(class) = self.u.class_symbol(widened) else { return false };
        REDECLARED
            .iter()
            .filter(|(_, m)| *m == name)
            .any(|(i, _)| self.resolve_full_name(i, true).is_some_and(|i| self.u.derives_from(class, i)))
    }

    /// The member of `Any` the JDK's `java.lang.Class` inherits where nativelib's `_Class`
    /// overrides `sym` (`equals`, `hashCode`); `sym` otherwise.
    fn jdk_inherited(&mut self, sym: SymbolId) -> SymbolId {
        let s = self.u.symbol(sym);
        let name = s.name.clone();
        let Some(owner) = s.owner.filter(|_| matches!(name.as_str(), "equals" | "hashCode")) else {
            return sym;
        };
        if self.u.full_name(owner) != "java.lang.Class" {
            return sym;
        }
        let any = self.u.builtins().any;
        self.u.decl(any, &name).unwrap_or_default().first().copied().unwrap_or(sym)
    }

    fn member_not_found(&mut self, id: NodeId, qual: TNodeId, name: TName, ctx: Ctx) -> TNodeId {
        self.not_a_member(id, qual, name, ctx).unwrap_or_else(|| self.unsupported(id, "member not found"))
    }
}
