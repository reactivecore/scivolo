// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The alternatives of a reference that is not applied which take the parameters of an expected
//! single abstract method type (dotty's `narrowByTypes` in the `SAMType` case of
//! `resolveOverloaded1`).

use super::compatible::{compatible, is_compatible};
use super::normalized::{mentions_params, normalized};
use super::poly_instances::poly_instance;
use super::{Resolution, UNDECIDED};
use crate::typer::{Pt, Typer};
use tasty::{MAX_IMPLEMENTED_ARITY, SymbolId, TypeId};

/// Why a method whose parameter or result types depend on its parameters is not compared.
const DEPENDENT: &str = "overloads with dependent parameters";
/// Why a method that takes fewer arguments by defaults is not compared with a function type.
const DEFAULTS: &str = "overloaded method with default arguments against a function type";

impl Resolution {
    /// Finds the methods of `alts` that apply to arguments of the parameter types of the expected
    /// type with a result fitting its result type, through implicit conversions if `views`; none
    /// if the expected type is no function or single abstract method type.
    pub fn narrow_by_sam(
        self,
        t: &mut Typer,
        alts: &[SymbolId],
        views: bool,
    ) -> Result<Option<Vec<SymbolId>>, &'static str> {
        let Some((params, result)) = self.sam_of_pt(t)? else { return Ok(None) };
        let mut taking = Vec::new();
        for &s in alts {
            let tp = self.alt_type(t, s);
            let before = t.constraint.attempt();
            let fits = self.applies_to_types(t, (s, tp), (&params, result), views);
            t.constraint.abandon(before);
            if fits? {
                taking.push(s);
            }
        }
        Ok(Some(taking))
    }

    /// Finds the parameter and result types of the expected type if it is a function or single
    /// abstract method type (dotty's `SAMType`).
    fn sam_of_pt(self, t: &mut Typer) -> Result<Option<(Vec<TypeId>, TypeId)>, &'static str> {
        let Pt::Type(p) = self.pt else { return Ok(None) };
        if let Some(parts) = (0..=MAX_IMPLEMENTED_ARITY).find_map(|n| t.function_parts(p, n)) {
            return Ok(Some(parts));
        }
        for n in 0..=MAX_IMPLEMENTED_ARITY {
            if let Some(sam) = t.sam_shape(p, n)? {
                return Ok(Some((sam.params, sam.result)));
            }
        }
        Ok(None)
    }

    /// Checks whether alternative `s` of type `tp` applies to arguments of types `args` with a
    /// result fitting `result` (dotty's `isApplicableMethodRef`), constraining type variables.
    fn applies_to_types(
        self,
        t: &mut Typer,
        (s, tp): (SymbolId, TypeId),
        (args, result): (&[TypeId], TypeId),
        views: bool,
    ) -> Result<bool, &'static str> {
        let instance = poly_instance(t, tp, s, false);
        let Some(m) = t.u.method(instance) else { return Ok(false) };
        let (binder, mut formals, rest) = (m.binder, m.param_types(), m.result);
        if formals.iter().chain([&rest]).any(|&f| mentions_params(t.u, f, binder, s)) {
            return Err(DEPENDENT);
        }
        let elem = formals.last().and_then(|&p| t.repeated_elem(p));
        if elem.is_some() {
            formals.pop();
        }
        if args.len() < formals.len() {
            return if t.has_defaults(s) { Err(DEFAULTS) } else { Ok(false) };
        }

        for (i, &a) in args.iter().enumerate() {
            let Some(f) = formals.get(i).copied().or(elem) else { return Ok(false) };
            let f = t.u.by_name(f).unwrap_or(f);
            if !compatible(t, a, f, views, self.ctx).ok_or(UNDECIDED)? {
                return Ok(false);
            }
        }
        let rest = normalized(t, rest, s, Pt::Type(result))?;
        is_compatible(t, rest, result, views, self.ctx).ok_or(UNDECIDED)
    }
}
