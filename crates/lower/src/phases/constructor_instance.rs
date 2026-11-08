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
    let Type::Poly { binder, params, result } = l.u.types().get(info).clone() else {
        let types = accessors.into_iter().map(|a| l.u.info(a)).collect();
        return (type_ref(l.u, c), types);
    };
    let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
    let mut his: Vec<TypeId> = Vec::new();
    for (i, (_, bounds)) in params.iter().enumerate() {
        let (lo, hi) = match *l.u.types().get(*bounds) {
            Type::Bounds(lo, hi) => (lo, hi),
            _ => (tasty::NOTHING, *bounds),
        };
        let hi = l.u.subst(hi, binder, &names[..i], &his);
        let inst = match mentions_param(l, hi, binder, (&names[i], i)) {
            true => l.u.subst(lo, binder, &names[..i], &his),
            false => hi,
        };
        his.push(inst);
    }
    for i in (0..his.len()).rev() {
        his[i] = l.u.subst(his[i], binder, &names, &his);
    }
    let method = l.u.subst(result, binder, &names, &his);
    let types = match l.u.types().get(method).clone() {
        Type::Method { params, .. } => params.into_iter().map(|(_, t)| t).collect(),
        _ => accessors.into_iter().map(|a| l.u.info(a)).collect(),
    };
    (crate::definitions::applied(l.u, c, his), types)
}

/// Whether `t` mentions parameter `name`, the `index`th, of type lambda `binder`.
fn mentions_param(l: &mut Lower, t: TypeId, binder: tasty::Binder, (name, index): (&str, usize)) -> bool {
    let mut found = false;
    l.u.map_type(t, &mut |u, x| {
        found |= match u.types().get(x) {
            Type::Local(b, n) => b.0 == binder.0 && n == name,
            Type::Param(b, i) => *b == binder && *i as usize == index,
            _ => false,
        };
        None
    });
    found
}

/// The element type `T` of repeated parameter type `<repeated>[T]`.
pub fn repeated_elem(l: &mut Lower, t: TypeId) -> Option<TypeId> {
    let Type::Applied(c, args) = l.u.types().get(t).clone() else { return None };
    let class = l.u.class_symbol(c)?;
    (args.len() == 1 && l.u.full_name(class) == "scala.<repeated>").then(|| args[0])
}
