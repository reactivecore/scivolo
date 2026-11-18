//! The references of a capture set that can be pickled where it occurs.

use crate::defn::type_ref;
use names::nme;
use rustc_hash::FxHashMap;
use tasty::{Binder, Kind, SymbolId, Type, TypeId, Universe};

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
    /// The method types being pickled, the set inside them.
    pub binders: &'a [Binder],
    /// The definitions enclosing the tree.
    pub open: &'a [SymbolId],
}

/// `info`, the type of a field of `class`, with the constructor parameters its capture sets name
/// replaced by their fields seen from `this`: `b: B^{C.this.a}` of `class C(val a: A^, val b: B^{a})`.
pub fn sets_on_fields(u: &mut Universe, info: TypeId, class: SymbolId) -> TypeId {
    let class_ref = type_ref(u, class);
    let this = u.intern_type(Type::This(class_ref));
    u.map_with_capture_sets(info, &mut |u, part| {
        let Type::Sym(None, p) = *u.ty(part) else { return None };
        let constructor = u.symbol(p).owner?;
        let is_param = u.symbol(p).kind == Kind::Param
            && u.symbol(constructor).name == nme::CONSTRUCTOR
            && u.symbol(constructor).owner == Some(class);
        if !is_param {
            return None;
        }
        let name = u.symbol(p).name.clone();
        let field = u.decls_named(class, &name).into_iter().find(|&f| u.symbol(f).kind == Kind::Val)?;
        Some(u.intern_type(Type::Sym(Some(this), field)))
    })
}

/// Determines whether capture-set annotation type `annot` refers to no `this`, parameter or
/// definition other than a static one, a parameter of `scope.def` or of a method type of the
/// scope, a member selected from such a path, a member or local of a definition among
/// `scope.open` or the `this` of a class among them
/// (`case it: (It[E]^{it})` in `def from[E](it: It[E]^)`, the flow cast of
/// `var cur: It[A]^{from} | Null`), neither in a super accessor, the `this` of `scope.own` or of a
/// class enclosing it, which mapping a type that it annotates leaves alone; of an enclosing class
/// only if seeing it from `scope.seen` keeps it (`other.Views` of an inner trait `Views`).
pub fn is_static_capture_set(u: &mut Universe, annot: TypeId, scope: &CaptureScope<'_>) -> bool {
    let &CaptureScope { own, def, seen, methods, binders, open } = scope;
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
            Type::Sym(Some(p), _) if !matches!(u.ty(p), Type::NoPrefix) => true,
            Type::Sym(_, s) => {
                let owner = u.symbol(s).owner;
                u.is_static_symbol(s) || owner.is_some_and(|o| Some(o) == def || local(o))
            }
            Type::Param(b, _) => methods.contains_key(&b) || binders.contains(&b),
            Type::Local(..) | Type::RecThis(_) | Type::Skolem(..) | Type::Var(_) => false,
            _ => true,
        };
        this.then_some(part)
    });
    is_static
}
