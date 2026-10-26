// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/PostInlineNativeInterop.scala
//! The types erasure drops that the native intrinsics' code needs: the type arguments of
//! `sizeOf`, `alignmentOf` and `stackalloc` (nscplugin's `PostInlineNativeInterop`).

use crate::Lower;
use typer::tpd::{TNodeId, Tree};

/// The `Intrinsics` methods whose type argument is their operand.
const OF_TYPE: [&str; 3] = ["sizeOf", "alignmentOf", "stackalloc"];

/// Records the types the intrinsics' calls in `root` need after erasure, by method reference.
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
        out
    });
    l.non_erased.extend(found);
}
