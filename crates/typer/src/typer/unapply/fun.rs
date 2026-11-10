// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The selection of `C.unapply` or `C.unapplySeq` (dotty's `trySelectUnapply`).

use super::super::implicits::Goal;
use super::super::{Pt, Typer};
use super::{UnapplyPattern, proto};
use crate::ctx::{Ctx, Mode};
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Span, TName, Tree};

impl UnapplyPattern {
    /// Selects `qual.unapply` for extractor `qual`, typed as an expression, a member or an
    /// extension method, or `qual.unapplySeq` if it has neither, an overloaded one applied to a
    /// value of the scrutinee's type; whether it is `unapplySeq`. Refuses extension methods
    /// beside an `unapplySeq` and an `unapply` an implicit view may give beside one.
    pub fn unapply_fun(&self, t: &mut Typer<'_>, qual: NodeId) -> Result<(TNodeId, bool), &'static str> {
        let unsupported = t.unsupported.clone();
        let ctx = self.ctx.with_mode(Mode::NONE);
        let ident = matches!(t.ast.tree(qual), Tree::Ident { .. });
        let typed = t.typed(qual, Pt::Any, if ident { ctx.with_mode(Mode::EXTRACTOR) } else { ctx });
        let tpe = t.tpd.tpe(typed);
        let tpe = t.widen_if_unstable(tpe);
        let member = t.members_found(tpe, "unapply", false).is_some();
        let extension = !member && t.extension_visible("unapply", ctx);
        let seq_found = !member
            && (t.members_found(tpe, "unapplySeq", false).is_some()
                || t.extension_visible("unapplySeq", ctx));
        if extension && seq_found {
            return Err("extension method");
        }
        if seq_found && !extension && view_may_supply_unapply(t, typed, ctx) {
            return Err("unapply of an implicit view beside unapplySeq");
        }

        let seq = !extension && seq_found;
        let span = t.ast.span(qual);
        let splice = t.ast.node(Tree::TypeTree);
        t.ast.with_span(splice, span);
        t.splices.insert(splice, typed);
        let name = TName { name: t.name(if seq { "unapplySeq" } else { "unapply" }), is_type: false };
        let select = t.ast.node(Tree::Select { qualifier: splice, name });
        t.ast.with_span(select, Span::synthetic(span.start(), span.end()));
        let text = t.text(name.name).to_owned();
        let pt = proto::scrutinee_proto(t, tpe, &text, self.scrutinee, ctx);
        let unapp = t.typed_select(select, splice, name, pt, ctx);
        if t.unsupported != unsupported {
            return Err("extractor outside the subset");
        }

        let chosen = t.tpd.sym(t.tpd.meth_part(unapp)).map(|s| t.u.symbol(s).name.as_str());
        if chosen != Some(t.text(name.name)) {
            return Err("not an extractor");
        }
        Ok((unapp, seq))
    }
}

/// Whether an implicit conversion or an extension method of an implicit value may give `qual`
/// a member `unapply`.
fn view_may_supply_unapply(t: &mut Typer<'_>, qual: TNodeId, ctx: Ctx) -> bool {
    let reported = t.diags.items.len();
    let (found, clean, _) = t.speculatively(|t| t.viewed(qual, Goal::Member("unapply"), ctx));
    t.retract_diags(reported);
    match found {
        Ok(None) if clean => false,
        Ok(Some(_)) => true,
        _ => {
            let site = t.u.widen(t.tpd.tpe(qual));
            let q = t.u.class_symbol(site);
            q.is_none_or(|q| t.may_view_to((q, site), "unapply", ctx) != Some(false))
        }
    }
}
