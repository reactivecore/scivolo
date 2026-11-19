// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Choosing among the alternatives a name denotes when it is not applied, by the expected type.

use super::Resolution;
use super::normalized::{is_method_ignoring_pt, normalized_compatible};
use super::poly_instances::mapped_info;
use super::resolve::result_is_method;
use crate::typer::{Pt, Typer};
use tasty::{SymbolId, TypeId, Universe};

/// Why no alternative or several are chosen for a reference that is not applied.
const OVERLOADED_REFERENCE: &str = "overloaded reference";

impl Resolution {
    /// Chooses the alternative of `syms` that the expected type selects when the name is not
    /// applied, as `val g: Int => Int = f` among `def f(x: Int): Int` and `def f(x: String)`:
    /// the most specific of those that fit the expected type as values or eta-expanded, first
    /// without implicit conversions, then with them; else of a single abstract method type the
    /// methods taking its parameters, or the only alternative that is no method; a tie broken by
    /// the alternatives whose results are no methods, then by those without default arguments.
    /// Without a single choice, the only parameterless alternative, else the first one with an
    /// empty parameter list (dotty's `resolveOverloaded` with a type that is no application,
    /// then `adaptOverloaded`'s `tryParameterless`).
    pub fn parameterless_alternative(
        self,
        t: &mut Typer,
        syms: &[SymbolId],
    ) -> Result<SymbolId, &'static str> {
        let mut found = Vec::new();
        for views in [false, true] {
            found = self.resolve_unapplied(t, syms, views)?;
            if !found.is_empty() {
                break;
            }
        }
        match found[..] {
            [s] => Ok(s),
            _ => try_parameterless(t, syms),
        }
    }

    /// Gives the type of alternative `s` seen from the prefix, instantiated if it is resolved
    /// with type arguments.
    pub fn alt_type(self, t: &mut Typer, s: SymbolId) -> TypeId {
        t.complete(s);
        let tpe = t.ref_type(self.prefix, s);
        let info = t.method_info(tpe).unwrap_or(tpe);
        mapped_info(t, info, s)
    }

    /// Finds the most specific of the alternatives of `alts` that fit, through implicit
    /// conversions if `views`; of several those whose results are no methods, else those without
    /// default arguments, if that drops some (dotty's `resolveOverloaded1`).
    fn resolve_unapplied(
        self,
        t: &mut Typer,
        alts: &[SymbolId],
        views: bool,
    ) -> Result<Vec<SymbolId>, &'static str> {
        let candidates = self.candidates(t, alts, views)?;
        let found = self.narrow_most_specific(t, &candidates, views)?;
        if found.len() <= 1 {
            return Ok(found);
        }

        for without_defaults in [false, true] {
            let kept: Vec<SymbolId> = alts
                .iter()
                .copied()
                .filter(|&a| {
                    if without_defaults { !t.has_defaults(a) } else { !result_is_method(&mut t.types(), a) }
                })
                .collect();
            if kept.len() == 1 {
                return Ok(kept);
            }
            if kept.len() > 1 && kept.len() < alts.len() {
                return self.resolve_unapplied(t, &kept, views);
            }
        }
        Ok(candidates)
    }

    /// Finds the alternatives of `alts` that fit the expected type normalized, without an
    /// expected type those that take no parameters if some do, as a member of their results may
    /// be selected (dotty's `SelectionProto`); if none, of a single abstract method type those
    /// that take its parameters, else the only one that is no method if there is one.
    fn candidates(
        self,
        t: &mut Typer,
        alts: &[SymbolId],
        views: bool,
    ) -> Result<Vec<SymbolId>, &'static str> {
        let mut compatible = Vec::new();
        for &s in alts {
            let tp = self.alt_type(t, s);
            if normalized_compatible(t, (s, tp), self.pt, views, self.ctx)? {
                compatible.push(s);
            }
        }
        if matches!(self.pt, Pt::Any) {
            let values: Vec<SymbolId> = compatible
                .iter()
                .copied()
                .filter(|&s| {
                    let tp = self.alt_type(t, s);
                    !takes_params(t.u, tp)
                })
                .collect();
            if !values.is_empty() {
                compatible = values;
            }
        }
        if !compatible.is_empty() {
            return Ok(compatible);
        }

        if let Some(taking) = self.narrow_by_sam(t, alts, views)? {
            return Ok(taking);
        }
        let values: Vec<SymbolId> = alts
            .iter()
            .copied()
            .filter(|&s| {
                let tp = self.alt_type(t, s);
                !is_method_ignoring_pt(t.u, tp)
            })
            .collect();
        Ok(if values.len() == 1 { values } else { Vec::new() })
    }
}

/// Checks whether alternative type `tp` is a method whose first clause takes explicit parameters.
fn takes_params(u: &Universe, tp: TypeId) -> bool {
    u.method(u.strip_poly(tp)).is_some_and(|m| !m.implicit && !m.params.is_empty())
}

/// Finds the only alternative of `alts` that takes no parameters, else the first whose only
/// parameter list is empty (dotty's `tryParameterless`).
fn try_parameterless(t: &mut Typer, alts: &[SymbolId]) -> Result<SymbolId, &'static str> {
    let infos: Vec<(SymbolId, TypeId)> = alts.iter().map(|&s| (s, t.u.info(s))).collect();
    let parameterless: Vec<SymbolId> =
        infos.iter().filter(|&&(_, i)| t.u.method(t.u.strip_poly(i)).is_none()).map(|&(s, _)| s).collect();
    if let [s] = parameterless[..] {
        return Ok(s);
    }
    let empty_list = |&&(_, i): &&(SymbolId, TypeId)| {
        let m = t.u.method(t.u.strip_poly(i));
        m.is_some_and(|m| m.params.is_empty() && t.u.method(t.u.strip_poly(m.result)).is_none())
    };
    infos.iter().find(empty_list).map(|&(s, _)| s).ok_or(OVERLOADED_REFERENCE)
}
