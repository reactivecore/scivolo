//! The references of a capture set that can be pickled where it occurs.

use rustc_hash::FxHashMap;
use tasty::{Binder, SymbolId, Type, TypeId, Universe};

/// Determines whether capture-set annotation type `annot` refers to no `this`, parameter or
/// definition other than a static one, a parameter of definition `def` or of a method type whose
/// parameters `defined` has, a member or local of a definition among `open` or the `this` of a
/// class among them (those enclosing the tree: `case it: (It[E]^{it})` in `def from[E](it: It[E]^)`,
/// the flow cast of `var cur: It[A]^{from} | Null`), neither in a super accessor, the `this` of
/// class `own` or of a class enclosing it, which mapping a type that it annotates leaves alone; of
/// an enclosing class only if seeing it from `seen`, the prefix and class of a super accessor's
/// member, keeps it (`other.Views` of an inner trait `Views`).
pub fn is_static_capture_set(
    u: &mut Universe,
    annot: TypeId,
    (own, def): (Option<SymbolId>, Option<SymbolId>),
    seen: Option<(TypeId, SymbolId)>,
    defined: &FxHashMap<Binder, Vec<SymbolId>>,
    open: &[SymbolId],
) -> bool {
    let open = if seen.is_some() { &[] } else { open };
    let local = |s: SymbolId| open.contains(&s);
    let mut is_static = true;
    u.map_type(annot, &mut |u, part| {
        let this = matches!(u.ty(part), Type::This(_));
        is_static &= match *u.ty(part) {
            Type::This(c) => match *u.ty(c) {
                Type::Package(_) | Type::PackageName(_) => true,
                _ => u.class_symbol(c).is_some_and(|c| {
                    let mut enclosing = std::iter::successors(own, |&o| u.symbol(o).owner);
                    let kept =
                        |u: &mut Universe| seen.is_none_or(|(p, o)| u.as_seen_from(part, p, o) == part);
                    u.is_static_owner(c)
                        || Some(c) == own
                        || local(c)
                        || (enclosing.any(|o| o == c) && kept(u))
                }),
            },
            Type::Sym(_, s) => {
                let owner = u.symbol(s).owner;
                u.is_static_symbol(s) || owner.is_some_and(|o| Some(o) == def || local(o))
            }
            Type::Param(b, _) => defined.contains_key(&b),
            Type::Local(..) | Type::RecThis(_) | Type::Skolem(..) | Type::Var(_) => false,
            _ => true,
        };
        this.then_some(part)
    });
    is_static
}
