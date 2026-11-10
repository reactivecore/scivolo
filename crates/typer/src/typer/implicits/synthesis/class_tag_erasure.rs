// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//! The class a synthesized `ClassTag[T]` or `Manifest[T]` names: whether `T` erases alike in
//! every instance and the class it erases to.

use crate::defn::ARRAY;
use crate::typer::Typer;
use tasty::{Erased, Kind, SymbolId, Type, TypeId, Universe};

const EMPTY_TUPLE: &str = "scala.Tuple$package$.EmptyTuple$";

/// Determines whether all values of `tpe` share one runtime class, seeing through opaque types
/// (dotty's `hasStableErasure`); not so for a type parameter or an abstract type member.
pub fn has_stable_erasure(t: &mut Typer<'_>, tpe: TypeId) -> bool {
    let tpe = translucent(t, tpe);
    if let Type::Applied(tycon, args) = t.u.types().get(tpe).clone()
        && t.referenced(tycon).is_some_and(|s| t.u.symbol(s).kind != Kind::Class)
    {
        return t.u.expand_upper(tycon, &args).is_some_and(|hi| hi != tpe && has_stable_erasure(t, hi));
    }
    match *t.u.types().get(tpe) {
        Type::And(a, b) | Type::Or(a, b, _) => has_stable_erasure(t, a) && has_stable_erasure(t, b),
        Type::Applied(inner, _)
        | Type::Refined(inner, ..)
        | Type::Annotated(inner, _)
        | Type::Flexible(inner) => has_stable_erasure(t, inner),
        Type::Constant(..) | Type::Str(_) => true,
        Type::Sym(..) | Type::TypeRef(..) | Type::TermRef(..) => match t.referenced(tpe) {
            Some(s) if t.u.symbol(s).kind == Kind::Class => true,
            Some(s) if t.u.symbol(s).kind.is_term() => {
                let widened = t.u.widen(tpe);
                widened != tpe && has_stable_erasure(t, widened)
            }
            _ => false,
        },
        _ => false,
    }
}

/// Finds a reference to the class `tpe` erases to, `tpe` seen through opaque types and with
/// generic tuples normalized: an array or a value class itself, else the class among those of the
/// sides of `tpe`, their base classes and `Object` that erases as `tpe` does.
pub fn erased_class_type(t: &mut Typer<'_>, tpe: TypeId) -> Option<TypeId> {
    let tpe = translucent(t, tpe);
    let tpe = t.normalized_tuple_type(tpe);
    if t.class_args(tpe, ARRAY).is_some() {
        return Some(tpe);
    }
    let mut classes = Vec::new();
    side_classes(t, tpe, &mut classes);
    if let [c] = classes[..]
        && t.u.underlying(c).is_some()
    {
        return Some(crate::defn::type_ref(t.u, c));
    }
    let tuple = |u: &mut Universe, c| matches!(u.full_name(c).as_str(), "scala.*:" | EMPTY_TUPLE);
    if classes.is_empty() || classes.iter().any(|&c| tuple(t.u, c)) {
        return None;
    }
    let erased = t.u.erase(tpe);
    let mut bases: Vec<SymbolId> = classes.iter().flat_map(|&c| t.u.linearization(c)).collect();
    bases.extend(t.resolve_full_name("java.lang.Object", true));
    let found = bases.into_iter().find(|&c| {
        let class = crate::defn::type_ref(t.u, c);
        t.u.underlying(c).is_none()
            && match &erased {
                Erased::Class(name) => t.u.binary_name(c) == *name,
                _ => t.u.erase(class) == erased,
            }
    })?;
    Some(crate::defn::type_ref(t.u, found))
}

/// Adds the classes of the sides of intersection or union `tpe`, or of `tpe`, to `out`.
fn side_classes(t: &mut Typer<'_>, tpe: TypeId, out: &mut Vec<SymbolId>) {
    let tpe = translucent(t, tpe);
    match *t.u.types().get(tpe) {
        Type::And(a, b) | Type::Or(a, b, _) => {
            side_classes(t, a, out);
            side_classes(t, b, out);
        }
        Type::Refined(p, ..) | Type::Annotated(p, _) => side_classes(t, p, out),
        _ => {
            let widened = t.u.widen(tpe);
            let widened = t.u.dealias(widened);
            out.extend(t.u.class_symbol(widened));
        }
    }
}

/// `tpe` dealiased, its match types reduced and its opaque types replaced by their right-hand
/// sides, as far as they go (dotty's `translucentSuperType`).
fn translucent(t: &mut Typer<'_>, tpe: TypeId) -> TypeId {
    let mut tpe = t.u.dealias(tpe);
    for _ in 0..16 {
        let next = match *t.u.types().get(tpe) {
            Type::Match { .. } => t.u.try_normalize(tpe),
            Type::Alias(a) => Some(a),
            _ => opaque_expansion(t, tpe),
        };
        match next {
            Some(n) if n != tpe => tpe = t.u.dealias(n),
            _ => break,
        }
    }
    tpe
}

/// The right-hand side of opaque type reference `tpe`, applied to its arguments.
fn opaque_expansion(t: &mut Typer<'_>, tpe: TypeId) -> Option<TypeId> {
    let (tycon, args) = match t.u.types().get(tpe).clone() {
        Type::Applied(tycon, args) => (tycon, args),
        _ => (tpe, Vec::new()),
    };
    let s = t.referenced(tycon)?;
    if t.u.symbol(s).kind != Kind::Type || !t.u.symbol(s).flags.has(tasty::tags::OPAQUE) {
        return None;
    }
    t.complete(s);
    let rhs = t.u.opaque_alias(s).unwrap_or_else(|| t.u.info(s));
    let rhs = match *t.u.types().get(rhs) {
        Type::Alias(a) => a,
        Type::Bounds(..) => return None,
        _ => rhs,
    };
    if args.is_empty() {
        return Some(rhs);
    }
    let applied = t.u.intern_type(Type::Applied(rhs, args));
    Some(t.u.beta_reduced(applied))
}
