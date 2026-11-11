// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeUtils.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala

use crate::Lower;
use crate::phases::type_tests::singletons::is_singleton;
use tasty::{Type, TypeId};

pub use tasty::defn::MAX_TUPLE_ARITY;
/// How many skolems and singletons `tuple_element_types_up_to` looks through at most.
const MAX_WIDENINGS: usize = 64;

/// The element types of tuple type `t` if they are known: a `*:` chain ending in `EmptyTuple`, or
/// a `TupleN` (dotty's `tupleElementTypes`).
pub fn tuple_element_types(l: &mut Lower, t: TypeId) -> Option<Vec<TypeId>> {
    tuple_element_types_up_to(l, t, usize::MAX)
}

/// `tuple_element_types`, stopping after `bound + 1` elements of a `*:` chain (dotty's
/// `tupleElementTypesUpTo`).
pub fn tuple_element_types_up_to(l: &mut Lower, mut t: TypeId, bound: usize) -> Option<Vec<TypeId>> {
    let mut out = Vec::new();
    let mut widenings = 0;
    while out.len() <= bound {
        widenings += 1;
        if widenings > MAX_WIDENINGS + out.len() {
            return None;
        }
        let dealiased = l.u.dealias(t);
        let normalized = l.u.normalized(dealiased);
        let tp = l.u.dealias(normalized);
        match *l.u.ty(tp) {
            Type::Applied(tycon, args) => {
                let args = l.u.types_of(args).to_vec();
                let name = l.u.class_symbol(tycon).map(|c| l.u.full_name(c))?;
                if name == "scala.*:" && args.len() == 2 {
                    out.push(args[0]);
                    t = args[1];
                } else if args.len() <= MAX_TUPLE_ARITY && name == format!("scala.Tuple{}", args.len()) {
                    out.extend(args);
                    return Some(out);
                } else {
                    return None;
                }
            }
            Type::Skolem(_, underlying) => t = underlying,
            _ if is_singleton(l, tp) => {
                if is_empty_tuple_ref(l, tp) {
                    return Some(out);
                }
                let widened = l.u.widen(tp);
                if widened == tp {
                    return None;
                }
                t = widened;
            }
            _ => return None,
        }
    }
    Some(out)
}

/// `t` with references to terms replaced by their underlying types, repeatedly, and by-name
/// types by their results (dotty's `widenTermRefExpr`).
pub fn widen_term_ref_expr(l: &mut Lower, mut t: TypeId) -> TypeId {
    while is_term_ref(l, t) {
        let Some(next) = l.u.widen_once(t) else { return t };
        t = l.u.by_name(next).unwrap_or(next);
    }
    t
}

fn is_term_ref(l: &mut Lower, t: TypeId) -> bool {
    match *l.u.ty(t) {
        Type::TermRef(..) => true,
        Type::Sym(_, s) => l.u.symbol(s).kind.is_term(),
        _ => false,
    }
}

/// Whether `t` refers to the object `EmptyTuple`.
fn is_empty_tuple_ref(l: &mut Lower, t: TypeId) -> bool {
    let s = match *l.u.ty(t) {
        Type::Sym(_, s) => Some(s),
        Type::TermRef(..) => l.u.resolve_ref(t),
        _ => None,
    };
    s.is_some_and(|s| l.u.is_module(s) && l.u.full_name(s) == tasty::defn::EMPTY_TUPLE)
}
