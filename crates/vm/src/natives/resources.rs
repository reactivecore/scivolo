//! The externs behind Scala Native's `EmbeddedResourceReader`, over the resource files of the
//! classpath instead of resources embedded at link time.

use super::NativeFn;
use crate::Vm;

/// The classpath's resources, listed on first use; each one's content is read on first use.
#[derive(Default)]
pub struct Resources {
    all: Option<Vec<Resource>>,
}

struct Resource {
    root: usize,
    /// The `/`-rooted path.
    path: Box<[u8]>,
    content: Option<Box<[u8]>>,
}

pub fn extern_fn(name: &str) -> Option<NativeFn> {
    let f: NativeFn = match name {
        "scalanative_resource_get_embedded_size" => |vm, _| Ok(list(vm).len() as u64),
        "scalanative_resource_get_path_ptr" => {
            |vm, args| Ok(path(vm, args[0]).map_or(0, |p| p.as_ptr() as u64))
        }
        "scalanative_resource_get_path_length" => {
            |vm, args| Ok(path(vm, args[0]).map_or(0, |p| p.len() as u64))
        }
        "scalanative_resource_get_content_ptr" => {
            |vm, args| Ok(content(vm, args[0]).map_or(0, |c| c.as_ptr() as u64))
        }
        "scalanative_resource_get_content_length" => {
            |vm, args| Ok(content(vm, args[0]).map_or(0, |c| c.len() as u64))
        }
        _ => return None,
    };
    Some(f)
}

fn list(vm: &mut Vm) -> &mut Vec<Resource> {
    if vm.resources.all.is_none() {
        let found = vm.world.resources();
        let all = found
            .into_iter()
            .map(|(root, rel)| Resource { root, path: format!("/{rel}").into_bytes().into(), content: None })
            .collect();
        vm.resources.all = Some(all);
    }
    vm.resources.all.get_or_insert_default()
}

fn path(vm: &mut Vm, id: u64) -> Option<&[u8]> {
    list(vm).get(id as u32 as usize).map(|r| &*r.path)
}

/// The content of resource `id`, read on first use; empty if unreadable or over 2 GiB, which a
/// `CInt` length cannot hold.
fn content(vm: &mut Vm, id: u64) -> Option<&[u8]> {
    let id = id as u32 as usize;
    let r = list(vm).get(id)?;
    if r.content.is_none() {
        let (root, rel) = (r.root, std::str::from_utf8(&r.path[1..]).unwrap_or("").to_owned());
        let data = vm.world.resource(root, &rel).filter(|d| d.len() <= i32::MAX as usize).unwrap_or_default();
        list(vm)[id].content = Some(data.into());
    }
    list(vm)[id].content.as_deref()
}
