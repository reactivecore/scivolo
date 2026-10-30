// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//! The traits `ContextFunctionN[-T1, …, -TN, +R]`, N ≥ 0, and `FunctionN`, N > 22, that dotty's
//! `Definitions` gives the `scala` package, each with an abstract `def apply(x$0: T1, …): R`, a
//! context function's a `using` clause.

use super::builtin::synthetic;
use super::{Kind, SymbolId, Universe};
use crate::tags::{CONTRAVARIANT, COVARIANT, TRAIT};
use crate::types::{Type, TypeId};
use crate::{ANY, NOTHING};

/// The largest arity of a function class the library defines (dotty's
/// `MaxImplementedFunctionArity`).
pub const MAX_IMPLEMENTED_ARITY: usize = 22;

/// Enters the context function traits up to `MAX_IMPLEMENTED_ARITY` into package `scala`,
/// extending `object`.
pub fn enter(u: &mut Universe, scala: SymbolId, object: TypeId) {
    for n in 0..=MAX_IMPLEMENTED_ARITY {
        enter_class(u, (scala, object), n, true);
    }
}

/// The arity of `FunctionN` or `ContextFunctionN` named `name` if the library lacks that class
/// (dotty's `isSyntheticFunction`), and whether it is a context function.
pub fn synthetic_arity(name: &str) -> Option<(usize, bool)> {
    let (rest, contextual) = match name.strip_prefix("Context") {
        Some(rest) => (rest, true),
        None => (name, false),
    };
    let digits = rest.strip_prefix("Function")?;
    let n: usize = digits.parse().ok().filter(|_| !digits.starts_with('0'))?;
    (n > MAX_IMPLEMENTED_ARITY).then_some((n, contextual))
}

/// Enters trait `FunctionN` or `ContextFunctionN` of `n` parameters into package `scala`,
/// extending `object` (dotty's `newFunctionNType`).
pub fn enter_class(
    u: &mut Universe,
    (scala, object): (SymbolId, TypeId),
    n: usize,
    contextual: bool,
) -> SymbolId {
    let name = if contextual { format!("ContextFunction{n}") } else { format!("Function{n}") };
    let class = synthetic(u, scala, &name, Kind::Class, &[TRAIT]);
    u.parents.insert(class, vec![object]);
    let bounds = u.types.intern(Type::Bounds(NOTHING, ANY));
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
    let info = u.types.intern(Type::Method { binder, params, result, implicit: contextual, contextual });
    let apply = synthetic(u, class, "apply", Kind::Def, &[]);
    u.infos.insert(apply, info);
    u.set_deferred(apply, true);
    class
}

#[cfg(test)]
mod tests {
    use super::synthetic_arity;

    #[test]
    fn synthesizes_only_above_the_library_arities() {
        assert_eq!(synthetic_arity("Function23"), Some((23, false)));
        assert_eq!(synthetic_arity("ContextFunction30"), Some((30, true)));
        assert_eq!(synthetic_arity("Function22"), None);
        assert_eq!(synthetic_arity("Function023"), None);
        assert_eq!(synthetic_arity("ImpureFunction23"), None);
    }
}
