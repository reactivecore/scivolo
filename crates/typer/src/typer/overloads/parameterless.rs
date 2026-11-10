// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Choosing among the alternatives a name denotes when it is not applied, by the expected type.

use super::Resolution;
use super::applicability::param_types;
use super::poly_instances::mapped_info;
use crate::typer::type_ops::TypeOps;
use crate::typer::{Pt, Typer};
use tasty::{Kind, MAX_IMPLEMENTED_ARITY, SymbolId, Type, TypeId, Universe};

/// Why a reference against a function or single abstract method type selects no alternative,
/// or several.
const AGAINST_FUNCTION: &str = "overloaded reference against a function type";

impl Resolution {
    /// Chooses the alternative of `syms` that the expected type selects when the name is not
    /// applied (dotty's `resolveOverloaded` without arguments, then `tryParameterless`): the only
    /// one without parameter lists whose type conforms, as `val n: Int = size` among `def size:
    /// Int` and `def size(i: Int): Int`; of several the only value more specific than each
    /// method, else the only one with an implicit clause only; against a function or single
    /// abstract method type the only method taking its parameters; else the only parameterless one.
    pub fn parameterless_alternative(
        self,
        t: &mut Typer,
        syms: &[SymbolId],
    ) -> Result<SymbolId, &'static str> {
        if let Pt::Type(p) = self.pt
            && let Some((args, _)) = (0..=MAX_IMPLEMENTED_ARITY).find_map(|n| t.function_parts(p, n))
        {
            return self.taking_params(t, syms, &args, Some(p));
        }

        let (mut found, mut implicit, mut parameterless) = (Vec::new(), Vec::new(), Vec::new());
        for &s in syms {
            t.complete(s);
            let tpe = t.ref_type(self.prefix, s);
            let info = t.method_info(tpe).unwrap_or(tpe);
            let info = mapped_info(t, info, s);
            if !matches!(t.u.types().get(info), Type::Method { .. } | Type::Poly { .. }) {
                parameterless.push(s);
            }
            let result = match *t.u.types().get(info) {
                Type::ByName(r) => r,
                Type::Poly { result, .. } => match t.u.types().get(result) {
                    Type::Method { params, implicit: false, .. } if !params.is_empty() => continue,
                    _ => return Err("overloaded reference with a polymorphic alternative"),
                },
                Type::Method { ref params, result, implicit: false, .. } if params.is_empty() => result,
                Type::Method { result, implicit: true, .. } => {
                    implicit.push((s, result));
                    continue;
                }
                Type::Method { .. } => continue,
                _ => info,
            };
            if result_fits(&mut t.types(), result, self.pt) {
                found.push(s);
            }
        }

        let with_implicits = !implicit.is_empty();
        if found.is_empty() {
            let fitting: Vec<(SymbolId, TypeId)> =
                implicit.into_iter().filter(|&(_, r)| result_fits(&mut t.types(), r, self.pt)).collect();
            if let Some(best) = most_specific_result(t.u, &fitting) {
                return Ok(best);
            }
            found = fitting.into_iter().map(|(s, _)| s).collect();
        }
        if let (Pt::Type(p), []) = (self.pt, &found[..])
            && let Some(sam) = (0..=MAX_IMPLEMENTED_ARITY).find_map(|n| t.sam_shape(p, n).ok().flatten())
        {
            return self.taking_params(t, syms, &sam.params, None);
        }
        if let ([], [s], false) = (&found[..], &parameterless[..], with_implicits) {
            return Ok(*s);
        }

        let values: Vec<SymbolId> =
            found.iter().copied().filter(|&s| t.u.symbol(s).kind != Kind::Def).collect();
        match (&found[..], &values[..]) {
            ([s], _) => Ok(*s),
            (_, [v]) if found.iter().all(|&m| m == *v || value_beats(t.u, *v, m)) => Ok(*v),
            _ => Err("overloaded reference"),
        }
    }

    /// Chooses the only value of `syms` conforming to function type `fun`, else the only method
    /// whose parameters take arguments of types `args` (dotty's `normalizedCompatible` against a
    /// function type, a value more specific than a method, and the `SAMType` case of
    /// `resolveOverloaded1`); the constraint unchanged.
    fn taking_params(
        self,
        t: &mut Typer,
        syms: &[SymbolId],
        args: &[TypeId],
        fun: Option<TypeId>,
    ) -> Result<SymbolId, &'static str> {
        let (mut fitting, mut values) = (Vec::new(), Vec::new());
        for &s in syms {
            t.complete(s);
            if t.u.symbol(s).kind != Kind::Def {
                let tpe = t.ref_type(self.prefix, s);
                values.extend(fun.filter(|&f| t.u.is_subtype(tpe, f)).map(|_| s));
                continue;
            }
            let before = t.constraint.attempt();
            let fits = match param_types(t, s, self.prefix, true) {
                Some((formals, false)) => {
                    formals.len() == args.len() && args.iter().zip(&formals).all(|(&a, &f)| t.conforms(a, f))
                }
                Some((formals, true)) if formals.len() != args.len() => false,
                _ => {
                    t.constraint.abandon(before);
                    return Err(AGAINST_FUNCTION);
                }
            };
            t.constraint.abandon(before);
            if fits {
                fitting.push(s);
            }
        }
        match (&values[..], &fitting[..]) {
            ([s], _) | ([], [s]) => Ok(*s),
            _ => Err(AGAINST_FUNCTION),
        }
    }
}

/// Finds the one of several methods `alts` with an implicit clause only whose result type, of
/// `alts` alongside, conforms to each other's and not they to it (dotty's `isAsSpecific` of
/// implicit methods), if all have the same owner.
fn most_specific_result(u: &mut Universe, alts: &[(SymbolId, TypeId)]) -> Option<SymbolId> {
    let owner = u.symbol(alts.first()?.0).owner;
    if alts.len() < 2 || alts.iter().any(|&(s, _)| u.symbol(s).owner != owner) {
        return None;
    }
    let mut best = None;
    for &(s, r) in alts {
        let beats =
            alts.iter().filter(|&&(o, _)| o != s).all(|&(_, ro)| u.is_subtype(r, ro) && !u.is_subtype(ro, r));
        if beats {
            best = Some(s);
        }
    }
    best
}

/// Checks whether value `v` is more specific than `m`: `m` a method with an empty parameter list
/// of `v`'s class or a base class of it.
fn value_beats(u: &mut Universe, v: SymbolId, m: SymbolId) -> bool {
    let info = u.info(m);
    let empty = matches!(u.types().get(info), Type::Method { params, .. } if params.is_empty());
    let (Some(a), Some(b)) = (u.symbol(v).owner, u.symbol(m).owner) else { return false };
    u.symbol(m).kind == Kind::Def && empty && u.derives_from(a, b)
}

/// Checks whether an alternative of result type `result` is a value that may conform to `pt`.
fn result_fits(types: &mut TypeOps, result: TypeId, pt: Pt) -> bool {
    if matches!(types.u.types().get(result), Type::Method { .. } | Type::Poly { .. }) {
        return false;
    }
    match pt {
        Pt::Type(p) => {
            !types.is_fully_defined(p) || {
                let p = types.zonk(p);
                types.u.is_subtype(result, p)
            }
        }
        _ => true,
    }
}
