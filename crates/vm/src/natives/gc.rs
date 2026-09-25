//! Overrides of the `scalanative_GC_*` interface; the rest is Immix in the C runtime.

use super::NativeFn;

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    let f: NativeFn = match name.strip_prefix("scalanative_GC_")? {
        "init" => |_, _| Ok(0),
        "set_weak_references_collected_callback" => |vm, args| {
            vm.set_weak_handler(args[0]);
            Ok(0)
        },
        _ => return None,
    };
    Some(f)
}
