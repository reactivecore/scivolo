//! The calls of a file whose inferred type arguments their arguments may tell.

use crate::tpd::{TNodeId, Tpd, Tree as T};
use rustc_hash::FxHashMap;

/// Maps each inferred type argument tree in `root` to the call it is a type argument of, with all
/// its argument lists: `id(b)` for the `T` of `id[T](b)`.
pub fn type_arg_calls(tpd: &Tpd, root: TNodeId) -> FxHashMap<TNodeId, TNodeId> {
    let mut calls = FxHashMap::default();
    let mut stack = vec![(root, false)];
    while let Some((n, is_fun)) = stack.pop() {
        let fun = match *tpd.tree(n) {
            T::Apply { fun, .. } | T::TypeApply { fun, .. } => Some(fun),
            _ => None,
        };
        stack.extend(tpd.children(n).into_iter().map(|c| (c, Some(c) == fun)));
        if is_fun || fun.is_none() {
            continue;
        }
        let mut f = n;
        loop {
            match *tpd.tree(f) {
                T::Apply { fun, .. } => f = fun,
                T::TypeApply { fun, args } => {
                    let inferred = |&&a: &&TNodeId| matches!(tpd.tree(a), T::TypeTree { inferred: true });
                    calls.extend(tpd.items(args).iter().filter(inferred).map(|&a| (a, n)));
                    f = fun;
                }
                _ => break,
            }
        }
    }
    calls
}
