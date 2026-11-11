// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The selection of `C.unapply` or `C.unapplySeq` (dotty's `trySelectUnapply`).

use super::super::implicits::Goal;
use super::super::{Pt, Typer};
use super::{UnapplyPattern, proto};
use crate::ctx::{Ctx, Mode};
use crate::tpd::TNodeId;
use names::nme;
use syntax::ast::{NodeId, Span, TName, Tree};
use syntax::diag::Level;

/// The refusal of an extractor whose selected method is not `unapply` or `unapplySeq`.
const NOT_AN_EXTRACTOR: &str = "not an extractor";

/// The refusal of an extractor without a member, extension or view `unapply` or `unapplySeq`.
const NO_EXTRACTOR_METHOD: &str = "extractor without unapply";

impl UnapplyPattern {
    /// Selects the extractor method of `qual` as `select_unapply` does or, if that fails with
    /// errors or `qual` has no `unapply` or `unapplySeq` and `qual` names a type alias of a
    /// class, that of the class's companion object, as `Block` for
    /// `type Block = Trees.Block[Int]`; the errors of the first try if both fail.
    pub fn unapply_fun(&self, t: &mut Typer<'_>, qual: NodeId) -> Result<(TNodeId, bool), &'static str> {
        let (reported, unsupported) = (t.diags.items.len(), t.unsupported.clone());
        let first = self.select_unapply(t, qual, true);
        let failed = match first {
            Ok(_) => has_errors_since(t, reported),
            Err(what) => what == NOT_AN_EXTRACTOR || what == NO_EXTRACTOR_METHOD,
        };
        if !failed || t.unsupported != unsupported {
            return first;
        }

        let first_diags = t.diags.items[reported..].to_vec();
        t.retract_diags(reported);
        let kept = t.diags.items.len();
        if let Some(companion) = self.followed_type_alias(t, qual)? {
            let second = self.select_unapply(t, companion, false);
            if second.is_ok() && !has_errors_since(t, kept) && t.unsupported == unsupported {
                return second;
            }
            t.retract_diags(kept);
            t.unsupported = unsupported;
        }
        if first == Err(NO_EXTRACTOR_METHOD) {
            return self.select_unapply(t, qual, false);
        }

        let completed = t.diags.items.split_off(kept);
        t.diags.items.truncate(reported);
        t.diags.items.extend(first_diags.into_iter().chain(completed));
        first
    }

    /// Selects `qual.unapply` for extractor `qual`, typed as an expression, a member or an
    /// extension method, or `qual.unapplySeq` if it has neither, an overloaded one applied to a
    /// value of the scrutinee's type; whether it is `unapplySeq` (dotty's `trySelectUnapply`).
    /// With `stop_without`, stops before the selection if `qual` names overloaded methods, or has
    /// neither, is no `Dynamic` and no implicit view may give it one. Refuses extension methods
    /// beside an `unapplySeq` and an `unapply` an implicit view may give beside one.
    fn select_unapply(
        &self,
        t: &mut Typer<'_>,
        qual: NodeId,
        stop_without: bool,
    ) -> Result<(TNodeId, bool), &'static str> {
        let unsupported = t.unsupported.clone();
        let ctx = self.ctx.with_mode(Mode::NONE);
        if stop_without && names_overloaded_methods(t, qual, ctx) {
            return Err(NO_EXTRACTOR_METHOD);
        }
        let ident = matches!(t.ast.tree(qual), Tree::Ident { .. });
        let typed = t.typed(qual, Pt::Any, if ident { ctx.with_mode(Mode::EXTRACTOR) } else { ctx });
        let tpe = t.tpd.tpe(typed);
        let tpe = t.widen_if_unstable(tpe);
        let member = t.members_found(tpe, nme::UNAPPLY, false).is_some();
        let extension = !member && t.extension_visible(nme::UNAPPLY, ctx);
        let seq_found = !member
            && (t.members_found(tpe, nme::UNAPPLY_SEQ, false).is_some()
                || t.extension_visible(nme::UNAPPLY_SEQ, ctx));
        if extension && seq_found {
            return Err("extension method");
        }
        if seq_found && !extension && view_may_supply(t, typed, nme::UNAPPLY, ctx) {
            return Err("unapply of an implicit view beside unapplySeq");
        }
        if stop_without
            && !(member || extension || seq_found)
            && t.unsupported == unsupported
            && !t.derives_dynamic(typed)
            && !view_may_supply(t, typed, nme::UNAPPLY, ctx)
            && !view_may_supply(t, typed, nme::UNAPPLY_SEQ, ctx)
        {
            return Err(NO_EXTRACTOR_METHOD);
        }

        let seq = !extension && seq_found;
        let span = t.ast.span(qual);
        let splice = t.ast.node(Tree::TypeTree);
        t.ast.with_span(splice, span);
        t.splices.insert(splice, typed);
        let name = TName { name: t.name(if seq { nme::UNAPPLY_SEQ } else { nme::UNAPPLY }), is_type: false };
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
            return Err(NOT_AN_EXTRACTOR);
        }
        Ok((unapp, seq))
    }
}

/// Whether errors were reported from `mark` on.
pub fn has_errors_since(t: &Typer<'_>, mark: usize) -> bool {
    t.diags.items[mark..].iter().any(|d| d.level == Level::Error)
}

/// Whether extractor `qual`, `C` or `p.C`, refers to several methods taking explicit arguments,
/// as `New` imported from `tpd` with `def New(tpt: Tree)` and `def New(tp: Type)`.
fn names_overloaded_methods(t: &mut Typer<'_>, qual: NodeId, ctx: Ctx) -> bool {
    let (prefix, name) = match *t.ast.tree(qual) {
        Tree::Ident { name } => (None, name),
        Tree::Select { qualifier, name } => (Some(qualifier), name),
        _ => return false,
    };
    let text = t.text(name.name).to_owned();
    let reported = t.diags.items.len();
    let (found, clean, _) = t.speculatively(|t| match prefix {
        None => t.find_ref(&text, false, ctx.with_mode(Mode::EXTRACTOR)).ok().flatten(),
        Some(p) => {
            let typed = t.typed(p, Pt::Any, ctx.with_mode(Mode::QUALIFIER));
            let tpe = t.tpd.tpe(typed);
            let tpe = t.widen_if_unstable(tpe);
            t.members_found(tpe, &text, false)
        }
    });
    let errors = has_errors_since(t, reported);
    t.retract_diags(reported);
    let Some(found) = found.filter(|_| clean && !errors) else { return false };
    found.syms.len() > 1 && found.syms.iter().all(|&s| t.takes_explicit_args(s))
}

/// Whether an implicit conversion or an extension method of an implicit value may give `qual`
/// a member `name`.
fn view_may_supply(t: &mut Typer<'_>, qual: TNodeId, name: &str, ctx: Ctx) -> bool {
    let reported = t.diags.items.len();
    let (found, clean, _) = t.speculatively(|t| t.viewed(qual, Goal::Member(name), ctx));
    t.retract_diags(reported);
    match found {
        Ok(None) if clean => false,
        Ok(Some(_)) => true,
        _ => {
            let site = t.u.widen(t.tpd.tpe(qual));
            let q = t.u.class_symbol(site);
            q.is_none_or(|q| t.may_view_to((q, site), name, ctx) != Some(false))
        }
    }
}
