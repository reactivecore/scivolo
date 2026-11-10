// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala

use super::implicits::Goal;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::TypeId;

impl Typer<'_> {
    /// `qual.unapply` for extractor `qual`, typed as an expression, a member or an extension
    /// method, or `qual.unapplySeq` if it has neither, an overloaded one applied to a value of
    /// scrutinee type `sel` (dotty's `trySelectUnapply`); whether it is `unapplySeq`.
    pub fn unapply_fun(
        &mut self,
        qual: NodeId,
        sel: TypeId,
        ctx: Ctx,
    ) -> Result<(TNodeId, bool), &'static str> {
        let unsupported = self.unsupported.clone();
        let ctx = ctx.with_mode(Mode::NONE);
        let ident = matches!(self.ast.tree(qual), Tree::Ident { .. });
        let typed = self.typed(qual, Pt::Any, if ident { ctx.with_mode(Mode::EXTRACTOR) } else { ctx });
        let tpe = self.tpd.tpe(typed);
        let tpe = self.widen_if_unstable(tpe);
        let member = self.members_found(tpe, "unapply", false).is_some();
        let extension = !member && self.extension_visible("unapply", ctx);
        let seq_found = !member
            && (self.members_found(tpe, "unapplySeq", false).is_some()
                || self.extension_visible("unapplySeq", ctx));
        if extension && seq_found {
            return Err("extension method");
        }
        if seq_found && !extension && self.view_may_supply_unapply(typed, ctx) {
            return Err("unapply of an implicit view beside unapplySeq");
        }
        let seq = !extension && seq_found;
        let span = self.ast.span(qual);
        let splice = self.ast.node(Tree::TypeTree);
        self.ast.with_span(splice, span);
        self.splices.insert(splice, typed);
        let name = TName { name: self.name(if seq { "unapplySeq" } else { "unapply" }), is_type: false };
        let select = self.ast.node(Tree::Select { qualifier: splice, name });
        self.ast.with_span(select, Span::synthetic(span.start(), span.end()));
        let pt = self.scrutinee_proto(tpe, self.text(name.name).to_owned(), sel, ctx);
        let unapp = self.typed_select(select, splice, name, pt, ctx);
        if self.unsupported != unsupported {
            return Err("extractor outside the subset");
        }
        let chosen = self.tpd.sym(self.tpd.meth_part(unapp)).map(|s| self.u.symbol(s).name.as_str());
        if chosen != Some(self.text(name.name)) {
            return Err("not an extractor");
        }
        Ok((unapp, seq))
    }

    /// Whether an implicit conversion or an extension method of an implicit value may give `qual`
    /// a member `unapply`.
    fn view_may_supply_unapply(&mut self, qual: TNodeId, ctx: Ctx) -> bool {
        let reported = self.diags.items.len();
        let (found, clean, _) = self.speculatively(|t| t.viewed(qual, Goal::Member("unapply"), ctx));
        self.retract_diags(reported);
        match found {
            Ok(None) if clean => false,
            Ok(Some(_)) => true,
            _ => {
                let site = self.u.widen(self.tpd.tpe(qual));
                let q = self.u.class_symbol(site);
                q.is_none_or(|q| self.may_view_to((q, site), "unapply", ctx) != Some(false))
            }
        }
    }
}
