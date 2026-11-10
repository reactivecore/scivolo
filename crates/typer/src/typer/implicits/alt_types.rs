// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The types of implicit candidates as their ranking compares them: givens with classes of their
//! own and given objects as their parents (dotty's `widenGiven` and `prepare` in `compare`).

use super::super::Typer;
use super::super::type_ops::TypeOps;
use super::candidates::Candidate;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

/// The type of candidate `c` as seen from its prefix, a by-name result widened, the result of
/// a given with a class of its own widened to the class's parents.
pub fn alt_type(t: &mut Typer<'_>, c: Candidate) -> TypeId {
    t.complete(c.sym);
    let tpe = t.types().ref_type(c.prefix, c.sym);
    let info = t.method_info(tpe).unwrap_or(tpe);
    let info = match *t.u.types().get(info) {
        Type::ByName(r) => r,
        _ => info,
    };
    widen_given(t.u, info, c.sym)
}

fn widen_given(u: &mut Universe, t: TypeId, alt: SymbolId) -> TypeId {
    match u.types().get(t).clone() {
        Type::Method { binder, params, result, implicit: true, contextual } => {
            let result = widen_given(u, result, alt);
            u.intern_type(Type::Method { binder, params, result, implicit: true, contextual })
        }
        Type::Poly { binder, params, result } => {
            let result = widen_given(u, result, alt);
            u.intern_type(Type::Poly { binder, params, result })
        }
        _ => match u.class_symbol(t) {
            Some(c) if is_co_defined_given(u, alt, c) => parents_of(u, t, c),
            _ => t,
        },
    }
}

/// Whether `alt` is the method of a given or implicit class `cls` (dotty's `isCoDefinedGiven`).
fn is_co_defined_given(u: &Universe, alt: SymbolId, cls: SymbolId) -> bool {
    let (a, k) = (u.symbol(alt), u.symbol(cls));
    let given = |s: &tasty::Symbol| s.is_given() || s.is_old_implicit();
    a.kind == Kind::Def && given(a) && given(k) && a.name == k.name && a.owner == k.owner
}

/// `tpe`, or if it is the class of a given object, that class's parents.
pub fn prepared(ty: &mut TypeOps<'_>, tpe: TypeId) -> TypeId {
    let tpe = ty.zonk(tpe);
    let u = &mut *ty.u;
    let Type::Sym(_, c) = *u.types().get(tpe) else { return tpe };
    let given_object = u.is_module_class(c) && u.source_module(c).is_some_and(|m| u.symbol(m).is_given());
    if given_object { parents_of(u, tpe, c) } else { tpe }
}

/// The intersection of the parents of class `c` as base types of `t` (dotty's `widenToParents`).
pub fn parents_of(u: &mut Universe, t: TypeId, c: SymbolId) -> TypeId {
    let mut out: Option<TypeId> = None;
    for p in u.parents(c) {
        let base = u.class_symbol(p).and_then(|pc| u.base_type(t, pc)).unwrap_or(p);
        out = Some(out.map_or(base, |o| u.intern_type(Type::And(o, base))));
    }
    out.unwrap_or(t)
}
