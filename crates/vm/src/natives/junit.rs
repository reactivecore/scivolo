//! Externs through which `tests/junit/runner.scala` finds the JUnit bootstrapper modules.

use super::NativeFn;
use crate::Vm;
use crate::heap;
use crate::interp::Flow;

const SUFFIX: &str = "$scalanative$junit$bootstrapper$";

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    let f: NativeFn = match name {
        "scalai_bootstrapper_count" => |vm, _| Ok(bootstrappers(vm).len() as u64),
        "scalai_bootstrapper" => module,
        "scalai_bootstrapper_name" => name_of,
        _ => return None,
    };
    Some(f)
}

fn bootstrappers(vm: &mut Vm) -> &[String] {
    if vm.junit.is_none() {
        vm.junit = Some(vm.world.tops_ending(SUFFIX));
    }
    vm.junit.as_deref().unwrap_or_default()
}

fn nth(vm: &mut Vm, args: &[u64]) -> String {
    bootstrappers(vm).get(args[0] as u32 as usize).cloned().unwrap_or_default()
}

/// The module instance of the `i`th bootstrapper.
fn module(vm: &mut Vm, args: &[u64]) -> Flow {
    let top = nth(vm, args);
    let g = vm.world.top(&top);
    Ok(vm.module(g)? as u64)
}

/// The name of the test class of the `i`th bootstrapper, as a C string.
fn name_of(vm: &mut Vm, args: &[u64]) -> Flow {
    let top = nth(vm, args);
    let class = top.strip_suffix(SUFFIX).unwrap_or(&top);
    let s = vm.heap.permanent(class.len() + 1);
    for (i, b) in class.bytes().enumerate() {
        heap::write(s + i, b);
    }
    Ok(s as u64)
}
