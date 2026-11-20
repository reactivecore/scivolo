// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Selections whose extension methods several imports supply (dotty's `tryExtension` with
//! `altImports`): each tried, the one that fits kept.

use super::idents::Found;
use super::unapply::has_errors_since;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, TName, Tree};

/// The refusal of an alternative whose attempt met a construct outside the subset.
const ALTERNATIVE_OUTSIDE: &str = "imported extension method alternative outside the subset";

impl Typer<'_> {
    /// Selection `id` of `name` from `qual` as a call of the extension methods of one of `alts`:
    /// each tried, the only one that fits kept, else the first; two that fit are ambiguous.
    /// Refuses alternatives whose attempt meets a construct outside the subset or changes the
    /// dictionary of by-name implicits.
    pub fn ext_alternatives_apply(
        &mut self,
        id: NodeId,
        qual: TNodeId,
        name: TName,
        alts: Vec<Found>,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let mut fits = Vec::new();
        for (k, alt) in alts.iter().enumerate() {
            if self.receivers_reject(qual, alt.clone(), ctx) {
                continue;
            }
            let reported = self.diags.items.len();
            let (fit, clean, linked) = self.speculatively(|t| {
                let call = t.preset_ext_apply(id, qual, name, alt, pt, ctx);
                !has_errors_since(t, reported) && !t.is_error(call)
            });
            self.retract_diags(reported);
            if !clean || linked {
                return self.unsupported(id, ALTERNATIVE_OUTSIDE);
            }
            if fit {
                fits.push(k);
            }
        }

        if let [a, b, ..] = fits[..] {
            let text = self.text(name.name).to_owned();
            let shown = |t: &mut Self, k: usize| match alts[k].prefix {
                Some(p) => format!("{}.{text}", t.types().show_type(p)),
                None => text.clone(),
            };
            let (first, second) = (shown(self, a), shown(self, b));
            let msg = format!(
                "Ambiguous extension methods:\nboth {first}\nand  {second}\nare possible expansions of {text}"
            );
            return self.error(id, msg);
        }
        let chosen = fits.first().copied().unwrap_or(0);
        self.preset_ext_apply(id, qual, name, &alts[chosen], pt, ctx)
    }

    /// `ext_method_apply` of the extension methods `alt` refers to, named as they are, not as an
    /// import renames them.
    pub fn preset_ext_apply(
        &mut self,
        id: NodeId,
        qual: TNodeId,
        name: TName,
        alt: &Found,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let own = self.u.symbol(alt.syms[0]).name.clone();
        let name = TName { name: self.name(&own), ..name };
        let fun = self.ast.node(Tree::Ident { name });
        self.preset_refs.insert(fun, alt.clone());
        self.ext_method_apply(id, fun, qual, pt, ctx)
    }
}
