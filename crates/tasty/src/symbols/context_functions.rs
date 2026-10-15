//! The traits `ContextFunctionN[-T1, …, -TN, +R]`, N ≥ 1, that dotty's `Definitions` gives the
//! `scala` package, each with an abstract `def apply(using x$0: T1, …): R`.

use super::builtin::synthetic;
use super::{Kind, SymbolId, Universe};
use crate::tags::{CONTRAVARIANT, COVARIANT, TRAIT};
use crate::types::{Type, TypeId};
use crate::{ANY, NOTHING};

/// The largest arity of a context function class.
const MAX_ARITY: usize = 22;

/// Enters the traits into package `scala`, extending `object`.
pub fn enter(u: &mut Universe, scala: SymbolId, object: TypeId) {
    let bounds = u.types.intern(Type::Bounds(NOTHING, ANY));
    for n in 1..=MAX_ARITY {
        let name = format!("ContextFunction{n}");
        let class = synthetic(u, scala, &name, Kind::Class, &[TRAIT]);
        u.parents.insert(class, vec![object]);
        let prefix = format!("scala${name}$$");
        let param = |u: &mut Universe, name: String, variance: u8| {
            let p = synthetic(u, class, &name, Kind::TypeParam, &[variance]);
            u.infos.insert(p, bounds);
            u.types.intern(Type::Sym(None, p))
        };
        let args: Vec<TypeId> = (1..=n).map(|i| param(u, format!("{prefix}T{i}"), CONTRAVARIANT)).collect();
        let result = param(u, format!("{prefix}R"), COVARIANT);
        let binder = u.fresh_binder();
        let params = args.iter().enumerate().map(|(i, &t)| (format!("x${i}"), t)).collect();
        let info = u.types.intern(Type::Method { binder, params, result, implicit: true, contextual: true });
        let apply = synthetic(u, class, "apply", Kind::Def, &[]);
        u.infos.insert(apply, info);
        u.set_deferred(apply, true);
    }
}
