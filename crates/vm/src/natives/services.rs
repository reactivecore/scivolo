//! `java.util.ServiceLoader.load`, which Scala Native's linker resolves: the providers of a
//! service are the classes its `META-INF/services` files on the classpath list, each
//! instantiated on first use.

use super::{Native, NativeFn};
use crate::interp::code::Num;
use crate::interp::{Flow, Func};
use crate::layout::ARRAY_HEADER;
use crate::{Vm, VmError, heap};
use nir::{DefnKind, GlobalId};
use std::rc::Rc;

pub fn method(owner: &str, sig: &str) -> Option<NativeFn> {
    let f: NativeFn = match (owner, sig) {
        ("java.util.ServiceLoader$", s) if is_load(s, 'O') => |vm, args| loader(vm, args[1]),
        ("java.util.ServiceLoader", s) if is_load(s, 'o') => |vm, args| loader(vm, args[0]),
        _ => return None,
    };
    Some(f)
}

fn is_load(sig: &str, scope: char) -> bool {
    (sig.starts_with("D4loadL15java.lang.Class") || sig.starts_with("D13loadInstalledL15java.lang.Class"))
        && sig.ends_with(&format!("L23java.util.ServiceLoaderE{scope}"))
}

/// A `ServiceLoader` of the providers of `service`, a class object.
fn loader(vm: &mut Vm, service: u64) -> Flow {
    if service == 0 {
        return vm.null_pointer();
    }
    let name = vm.class_name(vm.class_of_rtti(service as usize)).to_owned();
    let mut providers = Vec::new();
    for class in provider_names(vm, &name) {
        providers.push(provider(vm, &class)?);
    }
    let array = vm.link_top("scala.scalanative.runtime.ObjectArray")?;
    let array = vm.alloc_array(array, providers.len());
    for (i, &p) in providers.iter().enumerate() {
        heap::write(array + ARRAY_HEADER as usize + i * 8, p);
    }
    let cls = vm.link_top("java.util.ServiceLoader")?;
    let loader = vm.alloc_object(cls);
    let init =
        vm.world.member(vm.class(cls).name, "RL15java.lang.ClassLAL32java.util.ServiceLoader$Provider_E");
    vm.call_global(init, &[loader as u64, service, array as u64])?;
    Ok(loader as u64)
}

/// The provider classes the classpath's `META-INF/services/<service>` files list, in order,
/// without duplicates and `#` comments.
fn provider_names(vm: &mut Vm, service: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for file in vm.world.classpath_files(&format!("META-INF/services/{service}")) {
        for line in String::from_utf8_lossy(&file).lines() {
            let name = line.split('#').next().unwrap_or("").trim();
            if !name.is_empty() && !names.iter().any(|n| n == name) {
                names.push(name.to_owned());
            }
        }
    }
    names
}

/// A `ServiceLoader.Provider` of `class`, whose `get` makes a new instance.
fn provider(vm: &mut Vm, class: &str) -> Flow {
    let g = vm.world.top(class);
    if !matches!(vm.world.defn(g).map(|d| matches!(d.kind, DefnKind::Class { .. })), Ok(true)) {
        let message = format!("provider {class} not found");
        return vm.throw_new_with("java.util.ServiceConfigurationError", &message);
    }
    let cls = vm.link(g)?;
    let f = vm.funcs.all.len() as u32;
    vm.funcs.all.push(Func::Native { name: g, native: Rc::new(Native::New(g)) });
    let load = vm
        .callbacks
        .address(f, &[], Num::I64)
        .ok_or_else(|| VmError::Unsupported("no free callback for a service provider".into()))?;
    let module = vm.world.top("java.util.ServiceLoader$");
    let create = vm.world.member(
        module,
        "D23createIntrinsicProviderL15java.lang.ClassR_L32java.util.ServiceLoader$ProviderEO",
    );
    let module = vm.module(module)? as u64;
    vm.call_global(create, &[module, vm.class(cls).rtti as u64, load as u64])
}

impl Vm {
    /// A new instance of `class`, made with its constructor without parameters.
    pub fn instantiate(&mut self, class: GlobalId) -> Flow {
        let cls = self.link(class)?;
        let obj = self.alloc_object(cls) as u64;
        let init = self.world.member(class, "RE");
        self.call_global(init, &[obj])?;
        Ok(obj)
    }
}
