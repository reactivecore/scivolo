//! The references of a capture set that can be pickled where it occurs.

use rustc_hash::FxHashMap;
use tasty::{Binder, NameId, SymbolId, Type, TypeId, Universe};

/// What a capture set may refer to where it is pickled.
pub struct CaptureScope<'a> {
    /// The class whose `this` and enclosing classes' `this` the set may name.
    pub own: Option<SymbolId>,
    /// The definition whose parameters the set may name.
    pub def: Option<SymbolId>,
    /// The prefix and class of the super accessor's member pickled.
    pub seen: Option<(TypeId, SymbolId)>,
    /// The parameters of the methods pickled, by binder.
    pub methods: &'a FxHashMap<Binder, Vec<SymbolId>>,
    /// The parameter names of the method types being pickled, by binder.
    pub types: &'a FxHashMap<Binder, Vec<NameId>>,
    /// The definitions enclosing the tree.
    pub open: &'a [SymbolId],
}

/// Determines whether capture-set annotation type `annot` refers to no `this`, parameter or
/// definition other than a static one, a parameter of `scope.def` or of a method type of the
/// scope, a member or local of a definition among `scope.open` or the `this` of a class among them
/// (`case it: (It[E]^{it})` in `def from[E](it: It[E]^)`, the flow cast of
/// `var cur: It[A]^{from} | Null`), neither in a super accessor, the `this` of `scope.own` or of a
/// class enclosing it, which mapping a type that it annotates leaves alone; of an enclosing class
/// only if seeing it from `scope.seen` keeps it (`other.Views` of an inner trait `Views`).
pub fn is_static_capture_set(u: &mut Universe, annot: TypeId, scope: &CaptureScope<'_>) -> bool {
    let &CaptureScope { own, def, seen, methods, types, open } = scope;
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
            Type::Param(b, _) => methods.contains_key(&b) || types.contains_key(&b),
            Type::Local(..) | Type::RecThis(_) | Type::Skolem(..) | Type::Var(_) => false,
            _ => true,
        };
        this.then_some(part)
    });
    is_static
}
