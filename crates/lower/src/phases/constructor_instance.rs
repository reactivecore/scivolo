// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SyntheticMembers.scala

use super::case_members::case_accessors;
use crate::Lower;
use crate::definitions::type_ref;
use tasty::{SymbolId, Type, TypeId};

/// The type `new C[Ts]` creates and the types of the parameters of constructor `ctor` of case
/// class `c`, `Ts` the constructor's type parameters instantiated from above (dotty's
/// `instantiate(fromBelow = false)`): each its upper bound, its lower bound if the upper one
/// mentions it, the parameters before it instantiated.
pub fn instantiated_constructor(l: &mut Lower, c: SymbolId, ctor: SymbolId) -> (TypeId, Vec<TypeId>) {
    let accessors = case_accessors(l, c);
    let info = l.u.info(ctor);
    let Some((binder, names, bounds, result)) =
        l.u.poly(info).map(|p| (p.binder, p.names(), p.param_types(), p.result))
    else {
        let types = accessors.into_iter().map(|a| l.u.info(a)).collect();
        return (type_ref(l.u, c), types);
    };
    let mut his: Vec<TypeId> = Vec::new();
    for (i, &bounds) in bounds.iter().enumerate() {
        let (lo, hi) = l.u.bounds(bounds).unwrap_or((tasty::NOTHING, bounds));
        let hi = l.u.subst(hi, binder, &names[..i], &his);
        let inst = match mentions_param(l, hi, binder, (names[i], i)) {
            true => l.u.subst(lo, binder, &names[..i], &his),
            false => hi,
        };
        his.push(inst);
    }
    for i in (0..his.len()).rev() {
        his[i] = l.u.subst(his[i], binder, &names, &his);
    }
    let method = l.u.subst(result, binder, &names, &his);
    let types = l.u.method(method).map(|m| m.param_types());
    let types = types.unwrap_or_else(|| accessors.into_iter().map(|a| l.u.info(a)).collect());
    (crate::definitions::applied(l.u, c, his), types)
}

/// Whether `t` mentions parameter `name`, the `index`th, of type lambda `binder`.
fn mentions_param(
    l: &mut Lower,
    t: TypeId,
    binder: tasty::Binder,
    (name, index): (tasty::NameId, usize),
) -> bool {
    let mut found = false;
    l.u.map_type(t, &mut |u, x| {
        found |= match u.ty(x) {
            Type::Local(b, n) => b.0 == binder.0 && *n == name,
            Type::Param(b, i) => *b == binder && *i as usize == index,
            _ => false,
        };
        None
    });
    found
}

/// The element type `T` of repeated parameter type `<repeated>[T]`.
pub fn repeated_elem(l: &mut Lower, t: TypeId) -> Option<TypeId> {
    let (c, args) = l.u.applied(t).map(|(c, a)| (c, a.to_vec()))?;
    let class = l.u.class_symbol(c)?;
    (args.len() == 1 && l.u.full_name(class) == tasty::defn::REPEATED).then(|| args[0])
}
