//! The externs behind Scala Native's `EmbeddedResourceReader`, over the resource files of the
//! classpath instead of resources embedded at link time.

use super::NativeFn;
use crate::Vm;
use crate::heap;
use crate::interp::Flow;

/// Each resource's path and content in permanent memory, as address and length; read from the
/// classpath on first use.
#[derive(Default)]
pub struct Resources {
    all: Option<Vec<[(usize, u32); 2]>>,
}

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    let f: NativeFn = match name {
        "scalanative_resource_get_embedded_size" => |vm, _| Ok(resources(vm).len() as u64),
        "scalanative_resource_get_path_ptr" => |vm, args| part(vm, args[0], 0, |(addr, _)| addr as u64),
        "scalanative_resource_get_path_length" => |vm, args| part(vm, args[0], 0, |(_, len)| len as u64),
        "scalanative_resource_get_content_ptr" => |vm, args| part(vm, args[0], 1, |(addr, _)| addr as u64),
        "scalanative_resource_get_content_length" => |vm, args| part(vm, args[0], 1, |(_, len)| len as u64),
        _ => return None,
    };
    Some(f)
}

/// `get` of the path (`which` 0) or content (1) of resource `id`; 0 for an unknown id.
fn part(vm: &mut Vm, id: u64, which: usize, get: fn((usize, u32)) -> u64) -> Flow {
    Ok(resources(vm).get(id as u32 as usize).map_or(0, |r| get(r[which])))
}

fn resources(vm: &mut Vm) -> &[[(usize, u32); 2]] {
    if vm.resources.all.is_none() {
        let files = vm.world.resources();
        let mut all = Vec::with_capacity(files.len());
        for (path, content) in files {
            all.push([permanent(vm, path.as_bytes()), permanent(vm, &content)]);
        }
        vm.resources.all = Some(all);
    }
    vm.resources.all.as_deref().unwrap_or_default()
}

/// A copy of `bytes` in permanent memory, as address and length.
fn permanent(vm: &mut Vm, bytes: &[u8]) -> (usize, u32) {
    let addr = vm.heap.permanent(bytes.len().max(1));
    if !bytes.is_empty() {
        heap::copy(bytes.as_ptr() as usize, addr, bytes.len());
    }
    (addr, bytes.len() as u32)
}
