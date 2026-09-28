use super::{Flow, FuncId};
use crate::class::ClassId;
use crate::{Vm, VmError};
use nir::{DefnKind, GlobalId, StrId, Type, TypeId};

impl Vm {
    /// The instance of module `name`, constructed on first access; other threads wait until the
    /// constructing thread is done.
    pub fn module(&mut self, name: GlobalId) -> Flow<usize> {
        if let Some(&obj) = self.modules.get(&name) {
            if !self.threads.initializing.is_empty() {
                self.await_module(name);
            }
            return Ok(obj);
        }
        let cls = self.link(name)?;
        let obj = self.alloc_object_in(cls, crate::object::Space::Permanent);
        self.modules.insert(name, obj);
        let me = self.threads.sched.current;
        self.threads.initializing.insert(name, (me, Vec::new()));
        let result = self.construct_module(name, obj);
        if let Some((_, waiters)) = self.threads.initializing.remove(&name) {
            for id in waiters {
                self.threads.sched.wake_if(id, crate::threads::Wait::Monitor);
            }
        }
        result.map(|()| obj)
    }

    /// Module `name`, remembered in `cell` unless a module is being constructed.
    pub fn cached_module(&mut self, cell: &std::cell::Cell<u64>, name: GlobalId) -> Flow {
        let obj = self.module(name)? as u64;
        if self.threads.initializing.is_empty() {
            cell.set(obj);
        }
        Ok(obj)
    }

    fn construct_module(&mut self, name: GlobalId, obj: usize) -> Flow<()> {
        let init = self.world.member(name, "RE");
        if self.world.defn(init).is_ok() {
            self.call_global(init, &[obj as u64])?;
        }
        if self.world.pool.top_name(name) == "scala.scalanative.reflect.Reflect$" {
            self.register_reflective_classes()?;
        }
        Ok(())
    }

    /// Waits while another thread constructs module `name`.
    fn await_module(&mut self, name: GlobalId) {
        let me = self.threads.sched.current;
        while let Some((owner, waiters)) = self.threads.initializing.get_mut(&name) {
            if *owner == me {
                return;
            }
            waiters.push(me);
            if !self.block(crate::threads::Wait::Monitor, None) {
                return;
            }
        }
    }

    /// The implementation of method `sig` for the class of `obj`: a definition, else a native the
    /// VM provides for a method the class lacks.
    pub fn virtual_method(&mut self, obj: usize, sig: StrId) -> Flow<FuncId> {
        let cls = self.class_of(obj);
        if let Some(&f) = self.classes.vtables.get(&(cls, sig)) {
            return Ok(f);
        }
        let target = match self.resolve(cls, sig)? {
            Some(target) => target,
            None => {
                let sig = self.world.pool.str(sig).to_owned();
                let provided = self.world.member(self.class(cls).name, &sig);
                if self.native(provided).is_none() {
                    let what = format!("{}::{sig}", self.class_name(cls));
                    return Err(VmError::Unsupported(format!("no method {what}")).into());
                }
                provided
            }
        };
        let f = self.func(target)?;
        self.classes.vtables.insert((cls, sig), f);
        Ok(f)
    }

    /// The first concrete definition of `sig` in `cls`'s lookup order.
    fn resolve(&mut self, cls: ClassId, sig: StrId) -> Flow<Option<GlobalId>> {
        for id in self.lookup_order(cls) {
            if let Some(g) = self.defined(id, sig)? {
                return Ok(Some(g));
            }
        }
        Ok(None)
    }

    /// Where methods of `cls` are looked up: its class chain, then the traits of each class in
    /// the chain, later-mixed traits first.
    pub fn lookup_order(&self, cls: ClassId) -> Vec<ClassId> {
        let mut order = Vec::new();
        let mut c = Some(cls);
        while let Some(id) = c {
            order.push(id);
            c = self.class(id).parent;
        }
        let chain = order.clone();
        for id in chain {
            self.push_traits(id, &mut order);
        }
        order
    }

    fn push_traits(&self, id: ClassId, order: &mut Vec<ClassId>) {
        for &t in self.class(id).traits.iter().rev() {
            if !order.contains(&t) {
                order.push(t);
                self.push_traits(t, order);
            }
        }
    }

    fn defined(&mut self, id: ClassId, sig: StrId) -> Flow<Option<GlobalId>> {
        let owner = self.class(id).name;
        let g = GlobalId(self.world.pool.globals.intern(nir::Global::Member(owner, sig)));
        Ok(match self.world.defn(g) {
            Ok(d) if matches!(d.kind, DefnKind::Define { .. }) => Some(g),
            _ => None,
        })
    }

    /// The class values of type `ty` belong to, if it is a class or array type.
    pub fn class_of_type(&mut self, ty: TypeId) -> Flow<Option<ClassId>> {
        Ok(match self.world.pool.ty(ty).clone() {
            Type::Ref { name, .. } => Some(self.link(name)?),
            Type::Array { elem, .. } => Some(self.array_class(elem)?),
            _ => None,
        })
    }

    pub fn is_instance(&mut self, obj: usize, ty: TypeId) -> Flow<bool> {
        if obj == 0 {
            return Ok(false);
        }
        let Some(target) = self.class_of_type(ty)? else {
            return Err(VmError::Unsupported("type test against a non-class type".into()).into());
        };
        Ok(self.is_subclass(self.class_of(obj), target))
    }

    /// `obj` if it is null or an instance of class type `ty`, whose class `cell` caches;
    /// otherwise a thrown `ClassCastException`.
    pub fn cast(&mut self, obj: u64, ty: TypeId, cell: &std::cell::Cell<u64>) -> Flow {
        if obj == 0 {
            return Ok(0);
        }
        if cell.get() == 0 {
            let Some(target) = self.class_of_type(ty)? else { return Ok(obj) };
            cell.set(target as u64 + 1);
        }
        let (from, to) = (self.class_of(obj as usize), cell.get() as ClassId - 1);
        if !self.is_subclass(from, to) {
            let (from, to) = (self.class(from).rtti as u64, self.class(to).rtti as u64);
            self.throw_runtime("D14throwClassCastR_R_nEO", &[from, to])?;
        }
        Ok(obj)
    }
}
