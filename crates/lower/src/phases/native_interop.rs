// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/PostInlineNativeInterop.scala
//! The types erasure drops that the native intrinsics' code needs (nscplugin's
//! `PostInlineNativeInterop`).

use crate::Lower;
use crate::codegen::funcptr::is_func_ptr_apply;
use typer::tpd::{TNodeId, Tree};

/// The `Intrinsics` methods whose type argument is their operand.
const OF_TYPE: [&str; 3] = ["sizeOf", "alignmentOf", "stackalloc"];

/// Records the types the intrinsics' calls in `root` need after erasure, by method reference:
/// the type arguments of `sizeOf`, `alignmentOf` and `stackalloc`, the argument and result types
/// of `CFuncPtrN.apply`; by sequence literal, the types of the variable arguments of extern calls.
pub fn transform(l: &mut Lower, root: TNodeId) {
    let found = l.tpd.fold(Vec::new(), root, &mut |mut out, t| {
        if let Tree::TypeApply { fun, args } = *l.tpd.tree(t)
            && let Some(m) = l.tpd.sym(fun)
            && OF_TYPE.contains(&l.u.symbol(m).name.as_str())
            && l.u.symbol(m).owner.is_some_and(|o| l.u.full_name(o) == crate::codegen::intrinsics::INTRINSICS)
        {
            let types = l.tpd.items(args).iter().map(|&a| l.tpd.tpe(a)).collect();
            out.push((fun, (types, l.tpd.span(t).without_inlined_call())));
        }
        if let Tree::Apply { fun, args } = *l.tpd.tree(t)
            && let Some(m) = l.tpd.sym(fun)
            && let Some(owner) = l.u.symbol(m).owner
            && is_func_ptr_apply(&l.u.full_name(owner), &l.u.symbol(m).name)
        {
            let mut types: Vec<_> = l.tpd.items(args).iter().map(|&a| l.tpd.tpe(a)).collect();
            types.push(l.tpd.tpe(t));
            out.push((fun, (types, l.tpd.span(t).without_inlined_call())));
        }
        if let Tree::Apply { fun, args } = *l.tpd.tree(t)
            && l.tpd.sym(fun).is_some_and(|m| crate::symbols::is_extern(l.u, m))
            && let Some(&last) = l.tpd.items(args).last()
            && let seq = l.tpd.strip_typed(last)
            && let Tree::SeqLiteral { elems, .. } = *l.tpd.tree(seq)
        {
            let elems = l.tpd.items(elems).to_vec();
            let types = elems.into_iter().map(|e| l.u.widen(l.tpd.tpe(e))).collect();
            out.push((seq, (types, l.tpd.span(seq))));
        }
        out
    });
    l.non_erased.extend(found);
}
