// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Choosing among the implicit values of one group that offer an extension method for a
//! selection (dotty's `rank` and `disambiguate` of extension candidates).

use super::super::super::Typer;
use super::super::attempts::fitting;
use super::super::candidates::{Candidate, candidate_ref};
use super::super::ranking::most_specific;
use super::view_kinds::extension_members;
use super::{Goal, ViewSearch};
use tasty::{SymbolId, TypeId};

const AMBIGUOUS: &str = "ambiguous extension methods of implicit values";

impl ViewSearch<'_> {
    /// Picks the implicit value of `group` whose extension method named as the goal's member
    /// takes the tree, as `given ops: Ops` for `"ab".twice`: the only one, else the most specific,
    /// else the one whose method ranks above the others', else the one whose method's result fits
    /// the goal's expected type (dotty's `deepenProto` after an ambiguous search); fails when
    /// several remain.
    pub fn extension_container(
        &self,
        t: &mut Typer<'_>,
        group: &[Candidate],
    ) -> Result<Option<Candidate>, &'static str> {
        let Some(name) = self.goal.member() else { return Ok(None) };
        let found = fitting(t, group, |t, c| self.try_extension(t, c, name))?;
        match found[..] {
            [] => Ok(None),
            [c] => Ok(Some(c)),
            _ => match most_specific(t, &found, self.ctx) {
                Some(c) => Ok(Some(c)),
                None => {
                    let (best, _, _) = t.speculatively(|t| self.by_extension_methods(t, &found, name));
                    if let Some(c) = best {
                        return Ok(Some(c));
                    }
                    let Goal::Typed(_, pt) = self.goal else { return Err(AMBIGUOUS) };
                    let typed: Vec<Candidate> =
                        found.into_iter().filter(|&c| self.extension_result_fits(t, c, name, pt)).collect();
                    match typed[..] {
                        [c] => Ok(Some(c)),
                        _ => Err(AMBIGUOUS),
                    }
                }
            },
        }
    }

    /// Tells whether the extension methods `name` of container `c` are one taking no further
    /// clause whose result conforms to `pt`, tried without changing the constraint.
    fn extension_result_fits(&self, t: &mut Typer<'_>, c: Candidate, name: &str, pt: TypeId) -> bool {
        let at = t.tpd.span(self.tree).start_pos();
        let (fits, clean, _) = t.speculatively(|t| {
            let Ok(container) = candidate_ref(t, c, at, self.ctx) else { return false };
            let value = t.tpd.tpe(container);
            let Ok(members) = extension_members(t, value, name) else { return false };
            let [m] = members[..] else { return false };
            let tpe = t.ref_type(Some(value), m);
            let info = t.method_info(tpe).unwrap_or(tpe);
            match t.u.method(info).map(|m| m.result) {
                Some(result) if t.u.method(result).is_none() => t.conforms(result, pt),
                _ => false,
            }
        });
        fits && clean
    }

    /// Picks the container of `found` whose extension method `name` ranks above the others'
    /// (dotty's fallback in `disambiguate` comparing the extension methods instead of their
    /// wrappers).
    fn by_extension_methods(&self, t: &mut Typer<'_>, found: &[Candidate], name: &str) -> Option<Candidate> {
        let at = t.tpd.span(self.tree).start_pos();
        let mut methods = Vec::new();
        for &c in found {
            let container = candidate_ref(t, c, at, self.ctx).ok()?;
            let container = t.with_type_vars(container).ok()?;
            let tpe = t.tpd.tpe(container);
            let info = t.method_info(tpe).unwrap_or(tpe);
            let value = t.strip_implicit(info);
            let members = extension_members(t, value, name).ok()?;
            let owner = |t: &Typer<'_>, m: SymbolId| t.u.symbol(m).owner;
            let overridden = |t: &mut Typer<'_>, m| {
                members.iter().any(|&o| match (owner(t, o), owner(t, m)) {
                    (Some(x), Some(y)) => x != y && t.u.derives_from(x, y),
                    _ => false,
                })
            };
            let kept: Vec<SymbolId> = members.iter().copied().filter(|&m| !overridden(t, m)).collect();
            let [m] = kept[..] else { return None };
            methods.push(Candidate { sym: m, prefix: Some(value) });
        }
        let best = most_specific(t, &methods, self.ctx)?;
        found.iter().zip(&methods).find(|&(_, &m)| m == best).map(|(&c, _)| c)
    }
}
