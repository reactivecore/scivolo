// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Rejecting an implicit candidate before it is referenced and typed: one with explicit
//! parameters for a goal other than a function type, as `try_implicit` does, or one whose
//! declared result type cannot conform by the classes it and its type arguments refer to, type
//! parameters and uninstantiated type variables taken as wildcards within their bounds (after
//! dotty's `isCompatible` in `candidateKind`, `wildApprox`); eligibility by the value type
//! (dotty's `filterMatching`).

use super::super::Typer;
use super::alt_types::alt_type;
use super::as_good::relaxed_conforms;
use super::candidates::Candidate;
use super::function_goals::implicit_value_type;
use super::search::ImplicitSearch;
use crate::defn::PRODUCT;
use crate::symbols::Completion;
use tasty::{SymbolId, Type, TypeId, TypeVars, Universe, VarId};

/// How deep type arguments are compared.
const DEPTH: u32 = 4;
/// Classes whose types conform to or from others other than by their base classes.
const SPECIAL: [&str; 9] = [
    "scala.Any",
    "scala.AnyKind",
    "scala.AnyVal",
    "scala.Matchable",
    "java.lang.Object",
    "scala.Singleton",
    "scala.Nothing",
    "scala.Null",
    PRODUCT,
];

impl ImplicitSearch {
    /// Whether candidate `c` gives a value that may conform to `pt` (dotty's `filterMatching`
    /// for a value), the constraint unchanged.
    pub fn is_eligible(&self, t: &mut Typer<'_>, c: Candidate) -> bool {
        let before = t.constraint.attempt();
        let tpe = alt_type(t, c);
        let tpe = t.poly_instance(tpe, c.sym, false);
        let fits = implicit_value_type(t, tpe, c.sym, self.pt)
            .is_some_and(|v| relaxed_conforms(&mut t.types(), v, self.pt));
        t.constraint.abandon(before);
        fits
    }
}

/// Whether the value of implicit `sym` cannot conform to `pt`: it takes explicit parameters
/// and `pt` is no function type, or the class of its declared result type does not derive
/// from that of `pt`, or a type argument of its base type at that class cannot conform to the
/// corresponding one of `pt`.
pub fn cannot_fit(t: &mut Typer<'_>, sym: SymbolId, pt: TypeId) -> bool {
    let mut tpe = t.u.info(sym);
    loop {
        tpe = match *t.u.ty(tpe) {
            Type::Poly { result, .. }
            | Type::Method { result, implicit: true, .. }
            | Type::ByName(result) => result,
            Type::Method { .. } => return !t.types().is_function_type(pt),
            _ => break,
        };
    }
    let (Some((ca, _)), Some((cb, ba))) = (nominal(t, tpe), nominal(t, pt)) else { return false };
    if !related(t.u, ca, cb) {
        return true;
    }
    let tpe = t.u.dealias(tpe);
    let Some((_, aa)) = t.u.base_type(tpe, cb).and_then(|b| nominal(t, b)) else { return false };
    let variances = t.u.variances(cb);
    aa.len() == ba.len()
        && aa.iter().zip(&ba).enumerate().any(|(i, (&x, &y))| match (open_var(t, y), variances.get(i)) {
            (Some(v), Some(&variance)) => outside_bounds(t, x, v, variance),
            (Some(_), None) => false,
            (None, _) => unrelated(t, x, y, DEPTH),
        })
}

/// Whether `x` cannot be an argument of variance `variance` for type variable `v` within its
/// bounds.
fn outside_bounds(t: &mut Typer<'_>, x: TypeId, v: VarId, variance: i8) -> bool {
    let Some((cx, _)) = nominal(t, x) else { return false };
    let (lo, hi) = t.constraint.bounds(v);
    if variance <= 0
        && let Some((cl, _)) = nominal(t, lo)
        && !related(t.u, cl, cx)
    {
        return true;
    }
    variance >= 0 && nominal(t, hi).is_some_and(|(ch, _)| !related(t.u, cx, ch))
}

/// Whether `a` and `b` cannot be related by conformance in either direction.
fn unrelated(t: &mut Typer<'_>, a: TypeId, b: TypeId, depth: u32) -> bool {
    let (Some((ca, aa)), Some((cb, ba))) = (nominal(t, a), nominal(t, b)) else { return false };
    if ca != cb {
        return !related(t.u, ca, cb) && !related(t.u, cb, ca);
    }
    depth > 0 && aa.len() == ba.len() && aa.iter().zip(&ba).any(|(&x, &y)| unrelated(t, x, y, depth - 1))
}

/// Whether a type of class `sub` may conform to one of class `sup`: `sub` derives from `sup`,
/// or both are tuple classes other than two `TupleN` of different arities.
fn related(u: &mut Universe, sub: SymbolId, sup: SymbolId) -> bool {
    match (tuple_class(&u.full_name(sub)), tuple_class(&u.full_name(sup))) {
        (Some(Some(n)), Some(Some(m))) if n != m => false,
        (Some(_), Some(_)) => true,
        _ => u.derives_from(sub, sup),
    }
}

/// The type variable `tpe` is, if not instantiated.
fn open_var(t: &Typer<'_>, tpe: TypeId) -> Option<VarId> {
    t.u.var(tpe).filter(|&v| t.constraint.instance(v).is_none())
}

/// The class `tpe` refers to and its type arguments, if it is a class other than a special one
/// whose base classes are known.
fn nominal(t: &mut Typer<'_>, tpe: TypeId) -> Option<(SymbolId, Vec<TypeId>)> {
    let tpe = t.u.dealias(tpe);
    let (tycon, args) = t.u.applied(tpe).map_or((tpe, Vec::new()), |(tycon, args)| (tycon, args.to_vec()));
    let c = match *t.u.ty(tycon) {
        Type::Sym(_, s) => s,
        Type::TypeRef(..) => t.u.resolve_ref(tycon)?,
        _ => return None,
    };
    let full = t.u.full_name(c);
    let special = SPECIAL.contains(&full.as_str()) || full.starts_with("scala.ContextFunction");
    let known = matches!(t.syms.completions.get(&c), None | Some(Completion::Done))
        && !t.syms.incomplete.contains(&c);
    (t.u.symbol(c).is_class() && !special && known).then_some((c, args))
}

/// Whether `full` names a tuple class, with its arity if it is a `TupleN`.
fn tuple_class(full: &str) -> Option<Option<u32>> {
    let name = full.strip_prefix("scala.")?;
    match name {
        "Tuple" | "*:" | "EmptyTuple" | "EmptyTuple$" | "NonEmptyTuple" => Some(None),
        _ => name.strip_prefix("Tuple")?.parse().ok().map(Some),
    }
}
