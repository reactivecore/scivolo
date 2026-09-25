use super::{Flow, FuncId};
use crate::class::ClassId;
use crate::{Vm, VmError};
use nir::{DefnKind, GlobalId, StrId, Type, TypeId};

impl Vm {
    /// The instance of module `name`, constructed on first access.
    pub fn module(&mut self, name: GlobalId) -> Flow<usize> {
        if let Some(&obj) = self.modules.get(&name) {
            return Ok(obj);
        }
        let cls = self.link(name)?;
        let obj = self.alloc_object(cls);
        self.modules.insert(name, obj);
        let init = self.world.member(name, "RE");
        if self.world.defn(init).is_ok() {
            self.call_global(init, &[obj as u64])?;
        }
        Ok(obj)
    }

    /// The implementation of method `sig` for the class of `obj`.
    pub fn virtual_method(&mut self, obj: usize, sig: StrId) -> Flow<FuncId> {
        let cls = self.class_of(obj);
        if let Some(&f) = self.classes.vtables.get(&(cls, sig)) {
            return Ok(f);
        }
        let Some(target) = self.resolve(cls, sig)? else {
            let what = format!("{}::{}", self.class_name(cls), self.world.pool.str(sig));
            return Err(VmError::Unsupported(format!("no method {what}")).into());
        };
        let f = self.func(target)?;
        self.classes.vtables.insert((cls, sig), f);
        Ok(f)
    }

    /// The first concrete definition of `sig` along the class chain, then along the traits.
    fn resolve(&mut self, cls: ClassId, sig: StrId) -> Flow<Option<GlobalId>> {
        let mut chain = Vec::new();
        let mut c = Some(cls);
        while let Some(id) = c {
            chain.push(id);
            c = self.class(id).parent;
        }
        for &id in &chain {
            if let Some(g) = self.defined(id, sig)? {
                return Ok(Some(g));
            }
        }
        for &id in &chain {
            let mut seen = Vec::new();
            if let Some(g) = self.resolve_in_traits(id, sig, &mut seen)? {
                return Ok(Some(g));
            }
        }
        Ok(None)
    }

    fn resolve_in_traits(
        &mut self,
        id: ClassId,
        sig: StrId,
        seen: &mut Vec<ClassId>,
    ) -> Flow<Option<GlobalId>> {
        for t in self.class(id).traits.clone().into_iter().rev() {
            if seen.contains(&t) {
                continue;
            }
            seen.push(t);
            if let Some(g) = self.defined(t, sig)? {
                return Ok(Some(g));
            }
            if let Some(g) = self.resolve_in_traits(t, sig, seen)? {
                return Ok(Some(g));
            }
        }
        Ok(None)
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
}
