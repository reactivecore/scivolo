//! The aliases `ImpureFunctionN` and `ImpureContextFunctionN` that dotty's `Definitions` gives the
//! `scala` package: `FunctionN` and `ContextFunctionN` of the same parameters.

use super::builtin::synthetic;
use super::{Kind, SymbolId, Universe};
use crate::types::Type;
use crate::{ANY, NOTHING};

/// The largest arity of a function class.
const MAX_ARITY: usize = 22;

/// Enters the aliases into package `scala`.
pub fn enter(u: &mut Universe, scala: SymbolId) {
    let package = u.types.intern(Type::Package(scala));
    let bounds = u.types.intern(Type::Bounds(NOTHING, ANY));
    for base in ["Function", "ContextFunction"] {
        for n in 0..=MAX_ARITY {
            let class = u.types.intern(Type::TypeRef(package, format!("{base}{n}")));
            let binder = u.fresh_binder();
            let mut params: Vec<(String, _)> = (0..n).map(|i| (format!("X{i}"), bounds)).collect();
            params.push(("R".into(), bounds));
            let args = (0..=n).map(|i| u.types.intern(Type::Param(binder, i as u32))).collect();
            let result = u.types.intern(Type::Applied(class, args));
            let info = u.types.intern(Type::Lambda { binder, params, result });
            let alias = synthetic(u, scala, &format!("Impure{base}{n}"), Kind::Type, &[]);
            u.infos.insert(alias, info);
        }
    }
}
