// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Choosing among the implicit values of one group that offer an extension method for a
//! selection (dotty's `rank` and `disambiguate` of extension candidates).

use super::super::Typer;
use super::candidates::Candidate;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::SymbolId;

const AMBIGUOUS: &str = "ambiguous extension methods of implicit values";

impl Typer<'_> {
    /// The one candidate of `group` with an extension method `name` that takes `tree`, if any,
    /// else the most specific one; `Err` if none is, or if a conversion of the same group fits.
    pub fn extension_container(
        &mut self,
        group: &[Candidate],
        tree: TNodeId,
        name: &str,
        converts: bool,
        ctx: Ctx,
    ) -> Result<Option<Candidate>, &'static str> {
        let found = self.fitting(group, |t, c| t.try_extension(c, tree, name, ctx))?;
        match found[..] {
            [] => Ok(None),
            _ if converts => Err("implicit conversion beside an extension method of an implicit value"),
            [c] => Ok(Some(c)),
            _ => match self.most_specific(&found, ctx) {
                Some(c) => Ok(Some(c)),
                None => {
                    let at = self.tpd.span(tree).start_pos();
                    let (best, _, _) = self.speculatively(|t| t.by_extension_methods(&found, name, at, ctx));
                    best.map(Some).ok_or(AMBIGUOUS)
                }
            },
        }
    }

    /// The container of `found` whose extension method `name` ranks above the others' (dotty's
    /// fallback in `disambiguate` comparing the extension methods instead of their wrappers).
    fn by_extension_methods(
        &mut self,
        found: &[Candidate],
        name: &str,
        at: Span,
        ctx: Ctx,
    ) -> Option<Candidate> {
        let mut methods = Vec::new();
        for &c in found {
            let container = self.candidate_ref(c, at, ctx).ok()?;
            let container = self.with_type_vars(container).ok()?;
            let tpe = self.tpd.tpe(container);
            let info = self.method_info(tpe).unwrap_or(tpe);
            let value = self.strip_implicit(info);
            let members = self.extension_members(value, name).ok()?;
            let owner = |t: &Self, m: SymbolId| t.u.symbol(m).owner;
            let overridden = |t: &mut Self, m| {
                members.iter().any(|&o| match (owner(t, o), owner(t, m)) {
                    (Some(x), Some(y)) => x != y && t.u.derives_from(x, y),
                    _ => false,
                })
            };
            let kept: Vec<SymbolId> = members.iter().copied().filter(|&m| !overridden(self, m)).collect();
            let [m] = kept[..] else { return None };
            methods.push(Candidate { sym: m, prefix: Some(value) });
        }
        let best = self.most_specific(&methods, ctx)?;
        found.iter().zip(&methods).find(|&(_, &m)| m == best).map(|(&c, _)| c)
    }
}
