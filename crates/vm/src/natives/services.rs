//! `java.util.ServiceLoader.load`, which Scala Native's linker resolves; like a link without
//! configured service providers, every service gets an empty provider list.

use super::NativeFn;
use crate::Vm;
use crate::interp::Flow;

pub fn method(owner: &str, sig: &str) -> Option<NativeFn> {
    let f: NativeFn = match (owner, sig) {
        ("java.util.ServiceLoader$", s) if is_load(s, 'O') => |vm, args| empty_loader(vm, args[1]),
        ("java.util.ServiceLoader", s) if is_load(s, 'o') => |vm, args| empty_loader(vm, args[0]),
        _ => return None,
    };
    Some(f)
}

fn is_load(sig: &str, scope: char) -> bool {
    (sig.starts_with("D4loadL15java.lang.Class") || sig.starts_with("D13loadInstalledL15java.lang.Class"))
        && sig.ends_with(&format!("L23java.util.ServiceLoaderE{scope}"))
}

fn empty_loader(vm: &mut Vm, service: u64) -> Flow {
    let providers = vm.link_top("scala.scalanative.runtime.ObjectArray")?;
    let providers = vm.alloc_array(providers, 0);
    let cls = vm.link_top("java.util.ServiceLoader")?;
    let loader = vm.alloc_object(cls);
    let init =
        vm.world.member(vm.class(cls).name, "RL15java.lang.ClassLAL32java.util.ServiceLoader$Provider_E");
    vm.call_global(init, &[loader as u64, service, providers as u64])?;
    Ok(loader as u64)
}
