// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/Inliner.scala
//! The skolems a call's type holds for its arguments (dotty's `callValueSkolemss`).

use crate::typer::seen_from::normalized;
use crate::typer::stability::is_stable_path;
use tasty::{Binder, SymbolId, Type, TypeId, Universe};

/// Determines the skolem standing for each argument, of types `args`, whose parameter result type
/// `result` refers to, in type `actual` of an application: the one of the argument's widened type,
/// else of a type that conforms to, as retyping may narrow it to a constant. They are checked by
/// substituting them for parameters `params` of binder `binder` in `result`, as `applied_result`
/// does; `None` if that does not give a type equivalent to `actual`.
pub fn call_skolems(
    u: &mut Universe,
    (result, actual): (TypeId, TypeId),
    (binder, params): (Binder, &[SymbolId]),
    args: &[TypeId],
) -> Option<Vec<Option<TypeId>>> {
    let mut skolems = Vec::new();
    u.map_type(actual, &mut |u, part| {
        if matches!(u.ty(part), Type::Skolem(..)) && !skolems.contains(&part) {
            skolems.push(part);
        }
        None
    });
    let param_index = |u: &Universe, part: TypeId| match *u.ty(part) {
        Type::Param(b, i) if b == binder => Some(i as usize),
        Type::Sym(None, s) => params.iter().position(|&p| p == s),
        _ => None,
    };
    let mut used = vec![false; args.len()];
    u.map_type(result, &mut |u, part| {
        if let Some(flag) = param_index(u, part).and_then(|i| used.get_mut(i)) {
            *flag = true;
        }
        None
    });

    let mut chosen = Vec::new();
    for (&a, &used) in args.iter().zip(&used) {
        let w = u.widen(a);
        let n = normalized(u, w);
        let info = |u: &Universe, s: TypeId| match *u.ty(s) {
            Type::Skolem(_, i) => i,
            _ => s,
        };
        let mut found: Vec<TypeId> =
            skolems.iter().copied().filter(|&s| [w, n].contains(&info(u, s))).collect();
        if found.is_empty() {
            found = skolems.iter().copied().filter(|&s| u.is_subtype(w, info(u, s))).collect();
        }
        let unstable = !is_stable_path(u, a) || matches!(u.ty(a), Type::Constant(..) | Type::Str(_));
        chosen.push(match found[..] {
            [s] if used && unstable => Some(s),
            _ => None,
        });
    }

    let substituted = u.map_type(result, &mut |u, part| {
        let i = param_index(u, part)?;
        chosen.get(i).copied().flatten().or_else(|| args.get(i).copied())
    });
    let same =
        substituted == actual || u.is_subtype(substituted, actual) && u.is_subtype(actual, substituted);
    same.then_some(chosen)
}
