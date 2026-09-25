//! `dynmethod`: calls through structural types, via proxies like the ones Scala Native's linker
//! generates, which take and return boxed values.

use super::{Flow, Func, FuncId, func_ref};
use crate::class::ClassId;
use crate::{Vm, VmError};
use nir::{DefnKind, GlobalId, StrId, Type, TypeId, demangle};
use std::rc::Rc;

/// The target of a proxy and its parameter and result types, receiver first.
pub struct Proxy {
    pub target: FuncId,
    pub params: Box<[TypeId]>,
    pub ret: TypeId,
}

fn box_class(ty: &Type) -> Option<&'static str> {
    Some(match ty {
        Type::Bool => "java.lang.Boolean",
        Type::Char => "java.lang.Character",
        Type::Byte => "java.lang.Byte",
        Type::Short => "java.lang.Short",
        Type::Int => "java.lang.Integer",
        Type::Long => "java.lang.Long",
        Type::Float => "java.lang.Float",
        Type::Double => "java.lang.Double",
        Type::Size => "scala.scalanative.unsafe.Size",
        Type::Ptr => "scala.scalanative.unsafe.Ptr",
        _ => return None,
    })
}

impl Vm {
    /// A function reference to the proxy of method `proxy` for the class of `obj`.
    pub fn dyn_method(&mut self, obj: usize, proxy: StrId) -> Flow {
        if obj == 0 {
            return self.null_pointer();
        }
        let cls = self.class_of(obj);
        if let Some(&f) = self.funcs.proxies.get(&(cls, proxy)) {
            return Ok(func_ref(f));
        }
        let wanted = self.world.pool.str(proxy).to_owned();
        let Some(target) = self.find_proxied(cls, &wanted)? else {
            let what = format!("{}::{}", self.class_name(cls), demangle::sig(&wanted));
            return self.throw_new_with("java.lang.NoSuchMethodException", &what);
        };
        let defn = self.world.defn(target)?;
        let DefnKind::Define { ty, .. } = defn.kind else { unreachable!("found among defines") };
        let Type::Function(params, ret) = self.world.pool.ty(ty).clone() else {
            return Err(VmError::Unsupported("method without a function type".into()).into());
        };
        let proxy_fn = Proxy { target: self.func(target)?, params: params.into(), ret };
        let id = self.funcs.all.len() as FuncId;
        self.funcs.all.push(Func::Proxy { name: target, proxy: Rc::new(proxy_fn) });
        self.funcs.proxies.insert((cls, proxy), id);
        Ok(func_ref(id))
    }

    /// The public method of `cls`, in its lookup order, whose proxy signature is `wanted`.
    fn find_proxied(&mut self, cls: ClassId, wanted: &str) -> Flow<Option<GlobalId>> {
        for a in self.lookup_order(cls) {
            let owner = self.class(a).name;
            for m in self.world.members(owner)?.to_vec() {
                let sig = self.world.pool.member_sig(m).unwrap_or_default();
                let public = sig.ends_with('O');
                let proxy = demangle::to_proxy(sig);
                let is_define = matches!(self.world.defn(m)?.kind, DefnKind::Define { .. });
                if public && is_define && proxy.as_deref() == Some(wanted) {
                    return Ok(Some(m));
                }
            }
        }
        Ok(None)
    }

    /// Runs the static initializers that register classes with `scala.scalanative.reflect.Reflect`,
    /// which Scala Native runs at startup: those of every class with a generated
    /// `…$scalanative$ReflectivelyInstantiate$` companion.
    pub fn register_reflective_classes(&mut self) -> Flow<()> {
        const SUFFIX: &str = "$scalanative$ReflectivelyInstantiate$";
        for companion in self.world.tops_ending(SUFFIX) {
            let owner = self.world.top(companion.strip_suffix(SUFFIX).unwrap_or(&companion));
            if self.world.defn(owner).is_ok() {
                self.link(owner)?;
            }
        }
        Ok(())
    }

    /// Calls `proxy` with boxed `args`: unboxes them, calls the target and boxes its result.
    pub fn call_proxy(&mut self, proxy: &Proxy, args: &[u64]) -> Flow {
        let mut values = Vec::with_capacity(args.len());
        for (i, &a) in args.iter().enumerate() {
            let ty = proxy.params.get(i).copied();
            values.push(match ty {
                Some(ty) if i > 0 => self.boxing_of(ty, a, false)?,
                _ => a,
            });
        }
        let result = self.call(proxy.target, &values)?;
        match self.world.pool.ty(proxy.ret) {
            Type::Unit => Ok(self.unit()? as u64),
            _ => self.boxing_of(proxy.ret, result, true),
        }
    }

    /// `v` boxed (or unboxed) if `ty` is a primitive type, unchanged otherwise.
    fn boxing_of(&mut self, ty: TypeId, v: u64, boxing: bool) -> Flow {
        let Some(class) = box_class(self.world.pool.ty(ty)) else { return Ok(v) };
        let name = self.world.top(class);
        let boxed = TypeId(self.world.pool.types.intern(Type::Ref { name, exact: false, nullable: true }));
        let cells = [std::cell::Cell::new(0), std::cell::Cell::new(0)];
        self.boxed(boxed, boxing, v, &cells)
    }
}
