// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! The classes `FunctionN` and `ContextFunctionN`, and the classes erasure makes of them.

use super::casts::class_named;
use crate::Lower;
use tasty::{MAX_IMPLEMENTED_ARITY, SymbolId};

/// The arity of class `c` if it is `scala.FunctionN`, and whether it is `scala.ContextFunctionN`.
pub fn function_class_arity(l: &Lower, c: SymbolId) -> Option<(u32, bool)> {
    let full = l.u.full_name(c);
    let (rest, context) = match full.strip_prefix("scala.Context") {
        Some(rest) => (rest, true),
        None => (full.strip_prefix("scala.")?, false),
    };
    Some((rest.strip_prefix("Function")?.parse().ok()?, context))
}

/// Class `c` as erasure leaves it: `scala.FunctionN` for `scala.ContextFunctionN`,
/// `scala.runtime.FunctionXXL` for both above the library's arities (dotty's
/// `functionTypeErasure`).
pub fn erased_function_class(l: &mut Lower, c: SymbolId) -> Option<SymbolId> {
    match function_class_arity(l, c) {
        Some((n, _)) if n as usize > MAX_IMPLEMENTED_ARITY => class_named(l, "scala.runtime.FunctionXXL"),
        Some((n, true)) => class_named(l, &format!("scala.Function{n}")),
        _ => Some(c),
    }
}

/// Whether class `f` derives from `t` once erasure replaced the function classes.
pub fn erased_derives(l: &mut Lower, f: SymbolId, t: SymbolId) -> bool {
    l.u.derives_from(f, t) || erased_linearization(l, f).contains(&t)
}

/// The linearization of `class` with the function classes replaced as erasure leaves the parents.
pub fn erased_linearization(l: &mut Lower, class: SymbolId) -> Vec<SymbolId> {
    let bases = l.u.linearization(class);
    erased_classes(l, bases)
}

/// `classes` with the function classes replaced as erasure leaves them, first occurrences kept.
pub fn erased_classes(l: &mut Lower, classes: Vec<SymbolId>) -> Vec<SymbolId> {
    let mut out = Vec::new();
    for c in classes {
        let erased = erased_function_class(l, c).unwrap_or(c);
        if !out.contains(&erased) {
            out.push(erased);
        }
    }
    out
}
