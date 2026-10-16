//! `java.util.ServiceLoader.load`, which Scala Native's linker resolves: the providers of a
//! service are the classes its `META-INF/services` files on the classpath list, each
//! instantiated on first use.

use super::{Native, NativeFn};
use crate::class::ClassId;
use crate::interp::{Flow, Func, func_ref};
use crate::layout::ARRAY_HEADER;
use crate::{Vm, World, heap};
use nir::{Attr, DefnKind, GlobalId};
use std::rc::Rc;

pub fn method(owner: &str, sig: &str) -> Option<NativeFn> {
    let f: NativeFn = match (owner, sig) {
        ("java.util.ServiceLoader$", s) if is_load(s, 'O') => |vm, args| loader(vm, args[1]),
        ("java.util.ServiceLoader", s) if is_load(s, 'o') => |vm, args| loader(vm, args[0]),
        _ => return None,
    };
    Some(f)
}

/// Whether `sig` is `ServiceLoader.load` or `loadInstalled` of a service, in a module (`'O'`)
/// or static (`'o'`).
pub fn is_load(sig: &str, scope: char) -> bool {
    (sig.starts_with("D4loadL15java.lang.Class") || sig.starts_with("D13loadInstalledL15java.lang.Class"))
        && sig.ends_with(&format!("L23java.util.ServiceLoaderE{scope}"))
}

/// A `ServiceLoader` of the providers of `service`, a class object.
fn loader(vm: &mut Vm, service: u64) -> Flow {
    if service == 0 {
        return vm.null_pointer();
    }
    let service_class = vm.class_of_rtti(service as usize);
    let service_name = String::from(vm.class_name(service_class));
    let names = provider_names(&mut vm.world, &service_name);
    let array = vm.link_top("scala.scalanative.runtime.ObjectArray")?;
    let array = vm.alloc_array(array, names.len());
    for (i, name) in names.iter().enumerate() {
        let p = provider(vm, name, service_class)?;
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
pub fn provider_names(world: &mut World, service: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for file in world.classpath_files(&format!("META-INF/services/{service}")) {
        for line in String::from_utf8_lossy(&file).lines() {
            let name = line.split('#').next().unwrap_or("").trim();
            if !name.is_empty() && !names.iter().any(|n| n == name) {
                names.push(name.to_owned());
            }
        }
    }
    names
}

/// A `ServiceLoader.Provider` of `service` implemented by class `name`, whose `get` makes a new
/// instance, or throws `ServiceConfigurationError` if `name` is not a concrete subclass of
/// `service` with a constructor without parameters.
fn provider(vm: &mut Vm, name: &str, service: ClassId) -> Flow {
    let class = vm.world.top(name);
    let cls = match constructible(&mut vm.world, class) {
        true => vm.link(class)?,
        false => service,
    };
    let module = vm.world.top("java.util.ServiceLoader$");
    let create = vm.world.member(
        module,
        "D23createIntrinsicProviderL15java.lang.ClassR_L32java.util.ServiceLoader$ProviderEO",
    );
    let make = instantiator(vm, class, service);
    let module = vm.module(module)? as u64;
    vm.call_global(create, &[module, vm.class(cls).rtti as u64, func_ref(make)])
}

/// Whether `class` is a concrete class with a constructor without parameters.
pub fn constructible(world: &mut World, class: GlobalId) -> bool {
    let init = world.member(class, "RE");
    world.defn(class).is_ok_and(|d| {
        matches!(d.kind, DefnKind::Class { .. }) && !d.attrs.iter().any(|a| matches!(a, Attr::Abstract))
    }) && world.defn(init).is_ok()
}

/// The message of the `ServiceConfigurationError` of provider `provider` of `service` that is
/// not a concrete subclass with a constructor without parameters.
pub fn provider_error(service: &str, provider: &str) -> String {
    format!(
        "{service}: provider {provider} not found or not a concrete subclass with a constructor without parameters"
    )
}

/// The function that makes a new instance of `class` for `service`, created on first use.
fn instantiator(vm: &mut Vm, class: GlobalId, service: ClassId) -> u32 {
    if let Some(&f) = vm.funcs.instantiators.get(&(class, service)) {
        return f;
    }
    let f = vm.funcs.all.len() as u32;
    vm.funcs.all.push(Func::Native { name: class, native: Rc::new(Native::New(class, service)) });
    vm.funcs.instantiators.insert((class, service), f);
    f
}

/// A new instance of `class` made with its constructor without parameters, as a provider of
/// `service`.
pub fn instantiate(vm: &mut Vm, class: GlobalId, service: ClassId) -> Flow {
    let cls = match constructible(&mut vm.world, class) {
        true => Some(vm.link(class)?),
        false => None,
    };
    let Some(cls) = cls.filter(|&c| vm.is_subclass(c, service)) else {
        let message = provider_error(vm.class_name(service), vm.world.pool.top_name(class));
        return vm.throw_new_with("java.util.ServiceConfigurationError", &message);
    };
    let obj = vm.alloc_object(cls) as u64;
    let init = vm.world.member(class, "RE");
    vm.call_global(init, &[obj])?;
    Ok(obj)
}
