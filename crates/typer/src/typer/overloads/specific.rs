// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The most specific of alternatives a reference applied to no arguments may denote (dotty's
//! `narrowMostSpecific`, `compare`, `isAsGood` for overloading resolution).

use super::compatible::is_compatible;
use super::mapped::strip_implicit_clauses;
use super::poly_instances::poly_instance;
use super::{Resolution, UNDECIDED, applicability};
use crate::typer::Typer;
use crate::typer::implicits::alt_types::widen_given;
use std::cmp::Ordering;
use tasty::tags::EXTENSION;
use tasty::{Kind, SymbolId, Type, TypeId};

/// Why alternatives of unrelated objects are not compared: by their companion classes.
const OBJECT_OWNERS: &str = "overloaded reference to members of unrelated objects";
/// Why two values that are as good as each other are not compared: by their singleton types.
const EQUAL_VALUES: &str = "overloaded values as specific as each other";

/// An alternative with the type it is compared by.
type Typed = (SymbolId, TypeId);

impl Resolution {
    /// Keeps the alternatives of `alts` that no other is preferred over: the one preferred over
    /// all others, else it and those as good as it (dotty's `narrowMostSpecific`).
    pub fn narrow_most_specific(
        self,
        t: &mut Typer,
        alts: &[SymbolId],
        views: bool,
    ) -> Result<Vec<SymbolId>, &'static str> {
        let [first, rest @ ..] = alts else { return Ok(Vec::new()) };
        if let [second] = rest {
            return Ok(match self.compare(t, *first, *second, views)? {
                Ordering::Greater => vec![*first],
                Ordering::Less => vec![*second],
                Ordering::Equal => alts.to_vec(),
            });
        }

        let mut survivors = vec![*first];
        for &alt in rest {
            match self.compare(t, survivors[0], alt, views)? {
                Ordering::Greater => {}
                Ordering::Less => survivors[0] = alt,
                Ordering::Equal => survivors.insert(0, alt),
            }
        }
        let best = survivors[0];
        let mut found = vec![best];
        for &alt in &survivors[1..] {
            if self.compare(t, alt, best, views)? != Ordering::Less {
                found.push(alt);
            }
        }
        Ok(found)
    }

    /// Compares alternatives `a` and `b` by their owners and types with leading implicit clauses
    /// dropped, else by those clauses, fewer winning (dotty's `compare` without `preferGeneral`).
    fn compare(self, t: &mut Typer, a: SymbolId, b: SymbolId, views: bool) -> Result<Ordering, &'static str> {
        let (tp_a, tp_b) = (self.alt_type(t, a), self.alt_type(t, b));
        let (full_a, full_b) = (widen_given(t.u, tp_a, a), widen_given(t.u, tp_b, b));
        let (stripped_a, clauses_a) = strip_implicit_clauses(t.u, full_a, a);
        let (stripped_b, clauses_b) = strip_implicit_clauses(t.u, full_b, b);

        let by_stripped = self.compare_with_types(t, (a, stripped_a), (b, stripped_b), views)?;
        Ok(match (by_stripped, clauses_a, clauses_b) {
            (Ordering::Equal, 0, 0) => Ordering::Equal,
            (Ordering::Equal, 0, _) => Ordering::Greater,
            (Ordering::Equal, _, 0) => Ordering::Less,
            (Ordering::Equal, _, _) => self.compare_with_types(t, (a, full_a), (b, full_b), views)?,
            (decided, _, _) => decided,
        })
    }

    /// Compares alternatives by their owners, a subclass winning, and by types `ta` and `tb`
    /// (dotty's `compareWithTypes`).
    fn compare_with_types(
        self,
        t: &mut Typer,
        (a, ta): Typed,
        (b, tb): Typed,
        views: bool,
    ) -> Result<Ordering, &'static str> {
        let owner = compare_owner(t, a, b)?;
        let wins_a = self.as_good(t, (a, ta), (b, tb), views)?;
        let wins_b = self.as_good(t, (b, tb), (a, ta), views)?;
        if wins_a && wins_b && [a, b].iter().all(|&s| t.u.symbol(s).kind != Kind::Def) {
            return Err(EQUAL_VALUES);
        }
        Ok(match owner {
            Ordering::Greater if wins_a || !wins_b => Ordering::Greater,
            Ordering::Less if wins_b || !wins_a => Ordering::Less,
            Ordering::Equal => wins_a.cmp(&wins_b),
            _ => Ordering::Equal,
        })
    }

    /// Checks whether alternative `a` of type `ta` is as good as `b` of type `tb`: a method if
    /// it is nullary and `b` a method or `b` applies to its parameter types, a polymorphic one if
    /// its result is with abstract type parameters, a value against a method always, else if it
    /// conforms or converts to `b`'s type (dotty's `isAsGood`).
    fn as_good(
        self,
        t: &mut Typer,
        (a, ta): Typed,
        (b, tb): Typed,
        views: bool,
    ) -> Result<bool, &'static str> {
        match *t.u.ty(ta) {
            Type::Method { params, .. } => {
                let lambda = matches!(t.u.ty(tb), Type::Method { .. } | Type::Poly { .. });
                if t.u.params_of(params).is_empty() && lambda {
                    return Ok(true);
                }
                if t.u.symbol(a).flags.has(EXTENSION) && !t.u.symbol(b).flags.has(EXTENSION) {
                    return Ok(false);
                }
                self.applies_to_params(t, ta, b, views)
            }
            Type::Poly { .. } => {
                let instance = poly_instance(t, ta, a, true);
                self.as_good(t, (a, instance), (b, tb), views)
            }
            _ => {
                let value = t.u.by_name(ta).unwrap_or(ta);
                match *t.u.ty(tb) {
                    Type::Method { .. } => Ok(true),
                    Type::Poly { result, .. } if t.u.method(result).is_some() => Ok(true),
                    Type::Poly { .. } => {
                        let before = t.constraint.attempt();
                        let instance = poly_instance(t, tb, b, false);
                        let instance = t.u.by_name(instance).unwrap_or(instance);
                        let fits = is_compatible(t, value, instance, views, self.ctx);
                        t.constraint.abandon(before);
                        fits.ok_or(UNDECIDED)
                    }
                    _ => {
                        let other = t.u.by_name(tb).unwrap_or(tb);
                        is_compatible(t, value, other, views, self.ctx).ok_or(UNDECIDED)
                    }
                }
            }
        }
    }

    /// Checks whether method `b` applies to arguments of the parameter types of method type
    /// `ta`, a repeated one only if `b` takes a repeated parameter too (dotty's
    /// `isApplicableMethodRef` in `isAsGood`).
    fn applies_to_params(
        self,
        t: &mut Typer,
        ta: TypeId,
        b: SymbolId,
        views: bool,
    ) -> Result<bool, &'static str> {
        let Some(mut args) = t.u.method(ta).map(|m| m.param_types()) else { return Ok(false) };
        let elem = args.last().and_then(|&p| t.repeated_elem(p));
        if let Some(e) = elem {
            let b_repeated = applicability::param_types(t, b, self.prefix, true).is_some_and(|(_, r)| r);
            if !b_repeated {
                return Ok(false);
            }
            *args.last_mut().expect("a parameter") = e;
        }
        let info_b = t.u.info(b);
        if t.u.method(t.u.strip_poly(info_b)).is_none() {
            return Ok(false);
        }
        applicability::applicable(t, b, self.prefix, &args, views, self.ctx).ok_or(UNDECIDED)
    }
}

/// Compares the owners of `a` and `b`: a subclass is greater (dotty's `compareOwner`); refuses
/// unrelated objects, which dotty compares by their companion classes.
fn compare_owner(t: &mut Typer, a: SymbolId, b: SymbolId) -> Result<Ordering, &'static str> {
    let (Some(oa), Some(ob)) = (t.u.symbol(a).owner, t.u.symbol(b).owner) else { return Ok(Ordering::Equal) };
    if oa == ob {
        return Ok(Ordering::Equal);
    }
    if t.u.derives_from(oa, ob) {
        return Ok(Ordering::Greater);
    }
    if t.u.derives_from(ob, oa) {
        return Ok(Ordering::Less);
    }
    if t.u.is_module_class(oa) || t.u.is_module_class(ob) {
        return Err(OBJECT_OWNERS);
    }
    Ok(Ordering::Equal)
}
