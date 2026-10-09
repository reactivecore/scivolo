//! Selections `qual.name` (dotty's `typedSelect`).

use super::implicits::conversions::Goal;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, TName};
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    pub fn typed_select(&mut self, id: NodeId, qualifier: NodeId, name: TName, pt: Pt, ctx: Ctx) -> TNodeId {
        let mut qual = self.typed(qualifier, Pt::Any, ctx.with_mode(crate::ctx::Mode::NONE));
        if self.is_error(qual) {
            return self.error_tree(self.ast.span(id));
        }
        let text = self.text(name.name).to_owned();
        if text == "apply" && self.is_eta_closure(qual) {
            return self.unsupported(id, "apply of a method reference");
        }
        let (mut converted, mut widened) = (false, false);
        let (qual_tpe, syms) = loop {
            let qual_tpe = self.tpd.tpe(qual);
            let qual_tpe = self.widen_if_unstable(qual_tpe);
            let syms: Vec<SymbolId> = self
                .type_members(qual_tpe, &text)
                .into_iter()
                .filter(|&s| self.is_type_symbol(s) == name.is_type)
                .collect();
            if !syms.is_empty() || converted || name.is_type {
                break (qual_tpe, syms);
            }
            if matches!(pt, Pt::Fun(_))
                && let Some(proxy) = self.proxy_select(id, (qual, qual_tpe), name, pt, ctx)
            {
                return proxy;
            }
            if let Some(case) = self.widened_enum_case(qual) {
                qual = case;
                widened = true;
                continue;
            }
            if self.extension_visible(&text, ctx) {
                return self.unsupported(id, "extension method");
            }
            match self.converted(qual, Goal::Member(&text), ctx) {
                Ok(Some(q)) => qual = q,
                Ok(None) if !widened => return self.member_not_found(id, qual, name, ctx),
                Ok(None) => return self.unsupported(id, "member not found"),
                Err(what) => return self.unsupported(id, what),
            }
            converted = true;
        };
        if syms.is_empty() && name.is_type && !widened {
            return self.member_not_found(id, qual, name, ctx);
        }
        if syms.is_empty() {
            return self.unsupported(id, "member not found");
        }
        let sym = match self.choose(&syms, Some(qual_tpe), pt) {
            Ok(sym) => sym,
            Err(what) => return self.unsupported(id, what),
        };
        if let Err(what) = self.check_reference(sym, Some(qual_tpe), ctx) {
            return self.unsupported(id, what);
        }
        let flags = self.u.symbol(sym).flags;
        let protected = flags.has(tasty::tags::PROTECTED) || flags.has(tasty::tags::PROTECTEDqualified);
        let super_call = matches!(self.tpd.tree(qual), T::New { .. })
            && self.u.symbol(sym).name == "<init>"
            && self.u.symbol(ctx.owner).owner.is_some_and(|c| {
                self.syms.primary_ctors.get(&c) == Some(&ctx.owner)
                    && self.u.symbol(sym).owner.is_some_and(|p| self.u.derives_from(c, p))
            });
        if protected && !super_call && !matches!(self.u.types().get(qual_tpe), Type::This(_)) {
            return self.unsupported(id, "protected member through a path");
        }
        let projection = self.tpd.sym(qual).is_some_and(|q| self.is_type_symbol(q))
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

    fn member_not_found(&mut self, id: NodeId, qual: TNodeId, name: TName, ctx: Ctx) -> TNodeId {
        self.not_a_member(id, qual, name, ctx).unwrap_or_else(|| self.unsupported(id, "member not found"))
    }
}
