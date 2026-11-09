// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Choosing among the implicit values of one group that offer an extension method for a
//! selection (dotty's `rank` and `disambiguate` of extension candidates).

use super::super::Typer;
use super::attempts::fitting;
use super::candidates::{Candidate, candidate_ref};
use super::conversions::Goal;
use super::ranking::most_specific;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::{SymbolId, Type, TypeId};

const AMBIGUOUS: &str = "ambiguous extension methods of implicit values";

impl Typer<'_> {
    /// The one candidate of `group` with an extension method named as `goal`'s member that takes
    /// `tree`, else the most specific one, else the one whose method's result fits `goal`'s
    /// expected type (dotty's `deepenProto` after an ambiguous search); `Err` if none is.
    pub fn extension_container(
        &mut self,
        group: &[Candidate],
        tree: TNodeId,
        goal: Goal,
        ctx: Ctx,
    ) -> Result<Option<Candidate>, &'static str> {
        let Some(name) = goal.member() else { return Ok(None) };
        let found = fitting(self, group, |t, c| t.try_extension(c, tree, name, ctx))?;
        match found[..] {
            [] => Ok(None),
            [c] => Ok(Some(c)),
            _ => match most_specific(self, &found, ctx) {
                Some(c) => Ok(Some(c)),
                None => {
                    let at = self.tpd.span(tree).start_pos();
                    let (best, _, _) = self.speculatively(|t| t.by_extension_methods(&found, name, at, ctx));
                    if let Some(c) = best {
                        return Ok(Some(c));
                    }
                    let Goal::Typed(_, pt) = goal else { return Err(AMBIGUOUS) };
                    let typed: Vec<Candidate> = found
                        .into_iter()
                        .filter(|&c| self.extension_result_fits(c, tree, name, pt, ctx))
                        .collect();
                    match typed[..] {
                        [c] => Ok(Some(c)),
                        _ => Err(AMBIGUOUS),
                    }
                }
            },
        }
    }

    /// Whether the extension methods `name` of container `c` are one taking no further clause
    /// whose result conforms to `pt`, tried without changing the constraint.
    fn extension_result_fits(
        &mut self,
        c: Candidate,
        tree: TNodeId,
        name: &str,
        pt: TypeId,
        ctx: Ctx,
    ) -> bool {
        let at = self.tpd.span(tree).start_pos();
        let (fits, clean, _) = self.speculatively(|t| {
            let Ok(container) = candidate_ref(t, c, at, ctx) else { return false };
            let value = t.tpd.tpe(container);
            let Ok(members) = t.extension_members(value, name) else { return false };
            let [m] = members[..] else { return false };
            let tpe = t.ref_type(Some(value), m);
            let info = t.method_info(tpe).unwrap_or(tpe);
            match t.u.types().get(info).clone() {
                Type::Method { result, .. } if !matches!(t.u.types().get(result), Type::Method { .. }) => {
                    t.conforms(result, pt)
                }
                _ => false,
            }
        });
        fits && clean
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
            let container = candidate_ref(self, c, at, ctx).ok()?;
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
        let best = most_specific(self, &methods, ctx)?;
        found.iter().zip(&methods).find(|&(_, &m)| m == best).map(|(&c, _)| c)
    }
}
