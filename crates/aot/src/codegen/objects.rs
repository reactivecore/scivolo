//! Operations on objects: modules, allocation, fields, method lookup and type tests.

use super::body::FnBuilder;
use crate::R;
use crate::classes::{ClassId, Kind, is_virtual, method_key};
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Value, types};
use cranelift_module::Module;
use nir::{GlobalId, Op, StrId, ValId};

const BOXED_UNIT: &str = "scala.scalanative.runtime.BoxedUnit$";

impl FnBuilder<'_, '_> {
    pub fn object_op(&mut self, id: u64, op: &Op) -> R<Option<Value>> {
        if matches!(op, Op::Classalloc { .. } | Op::Arrayalloc { .. } | Op::Box { .. }) {
            self.non_null.insert(id);
        }
        Ok(Some(match op {
            Op::Module(g) => {
                let cls = self.reach.link(*g)?;
                let load = self.decls.module_load(self.module, self.reach, cls)?;
                let fref = self.module.declare_func_in_func(load, self.b.func);
                let call = self.b.ins().call(fref, &[]);
                self.b.inst_results(call)[0]
            }
            Op::Classalloc { name, zone } => {
                if zone.is_some() {
                    // Temporary until #115 (zones).
                    return Err("allocation in a zone is not supported yet".into());
                }
                let cls = self.reach.link(*name)?;
                self.alloc(cls)?
            }
            Op::Fieldload { ty, .. } if matches!(self.reach.world.pool.ty(*ty), nir::Type::Unit) => {
                self.b.ins().iconst(types::I8, 0)
            }
            Op::Fieldload { ty, obj, name } => {
                let (o, offset) = self.field(*obj, *name)?;
                let t = self.ty(*ty)?;
                self.b.ins().load(t, MemFlagsData::new(), o, offset)
            }
            Op::Fieldstore { ty, obj, name, value } => {
                let (o, offset) = self.field(*obj, *name)?;
                let v = match self.reach.world.pool.ty(*ty) {
                    nir::Type::Unit => self.boxed_unit()?,
                    _ => self.arg_of(*value, *ty)?,
                };
                self.b.ins().store(MemFlagsData::new(), v, o, offset);
                return Ok(None);
            }
            Op::Field { obj, name } => {
                let (o, offset) = self.field(*obj, *name)?;
                self.b.ins().iadd_imm_s(o, offset as i64)
            }
            Op::Method { obj, sig } => self.method(id, *obj, *sig)?,
            Op::Is { ty, obj } => {
                let o = self.val(*obj)?;
                self.is_instance(o, *ty)?
            }
            Op::Arrayalloc { ty, init, .. } => self.array_alloc(*ty, *init)?,
            Op::Arrayload { ty, arr, idx } => {
                let t = self.ty(*ty)?;
                let at = self.element(*ty, *arr, *idx)?;
                self.load(t, at, 0)
            }
            Op::Arraystore { ty, arr, idx, value } => {
                let at = self.element(*ty, *arr, *idx)?;
                let v = self.arg_of(*value, *ty)?;
                self.b.ins().store(MemFlagsData::new(), v, at, 0);
                return Ok(None);
            }
            Op::Arraylength(arr) => self.array_length(*arr)?,
            Op::Box { ty, obj } => self.boxing(*ty, *obj, true)?,
            Op::Unbox { ty, obj } => self.boxing(*ty, *obj, false)?,
            Op::As { ty, obj } => self.cast(*obj, *ty)?,
            // Temporary until #115 (`dynmethod`, aggregates).
            op => return Err(format!("operation not supported yet: {op:?}")),
        }))
    }

    /// A new instance of `cls` from the collector, with its header set.
    pub fn alloc(&mut self, cls: ClassId) -> R<Value> {
        self.reach.allocate(cls);
        let ptr = self.decls.ptr;
        let alloc = self.decls.import(self.module, "scivolo_aot_alloc", &[ptr, ptr], Some(ptr))?;
        let rtti = self.rtti_addr(cls)?;
        let size = self.b.ins().iconst(ptr, self.reach.classes.get(cls).size as i64);
        let fref = self.module.declare_func_in_func(alloc, self.b.func);
        let call = self.b.ins().call(fref, &[rtti, size]);
        Ok(self.b.inst_results(call)[0])
    }

    pub fn rtti_addr(&mut self, cls: ClassId) -> R<Value> {
        let data = self.decls.rtti(self.module, self.reach, cls)?;
        let gv = self.module.declare_data_in_func(data, self.b.func);
        Ok(self.b.ins().symbol_value(self.decls.ptr, gv))
    }

    /// The static class of value `v`, if it is an object.
    pub fn class_of_val(&mut self, v: ValId) -> R<Option<ClassId>> {
        Ok(Some(match self.reach.world.pool.val(v) {
            nir::Val::String(_) => self.reach.link_top("java.lang.String")?,
            nir::Val::ClassOf(_) => self.reach.link_top(crate::codegen::CLASS)?,
            nir::Val::Unit => self.reach.link_top(BOXED_UNIT)?,
            nir::Val::Null => return Ok(None),
            _ => {
                let ty = self.type_of(v)?;
                return self.reach.class_of_type(ty);
            }
        }))
    }

    /// The `BoxedUnit` instance, the value of `unit` where an object is expected.
    pub fn boxed_unit(&mut self) -> R<Value> {
        let cls = self.reach.link_top(BOXED_UNIT)?;
        let load = self.decls.module_load(self.module, self.reach, cls)?;
        let fref = self.module.declare_func_in_func(load, self.b.func);
        let call = self.b.ins().call(fref, &[]);
        Ok(self.b.inst_results(call)[0])
    }

    /// Object `obj`, checked not to be null, and the offset of its field `name`.
    fn field(&mut self, obj: ValId, name: GlobalId) -> R<(Value, i32)> {
        let o = self.val(obj)?;
        self.null_check_of(obj, o)?;
        Ok((o, self.field_offset(name)?))
    }

    fn field_offset(&mut self, name: GlobalId) -> R<i32> {
        let owner = self.reach.link(self.reach.world.owner(name))?;
        match self.reach.classes.get(owner).fields.get(&name) {
            Some(f) => Ok(f.offset as i32),
            None => Err(format!("no field {}", self.reach.world.name(name))),
        }
    }

    pub fn load(&mut self, t: types::Type, addr: Value, offset: i32) -> Value {
        self.b.ins().load(t, MemFlagsData::new(), addr, offset)
    }

    /// The address of method `sig` of `obj`: resolved here for modules and non-virtual methods,
    /// else from the vtable of its class or, if its static class has no slot, from the itable; a
    /// method of `null` throws NullPointerException.
    fn method(&mut self, id: u64, obj: ValId, sig: StrId) -> R<Value> {
        let ptr = self.decls.ptr;
        let Some(cls) = self.class_of_val(obj)? else {
            let ty = self.type_of(obj)?;
            let pool = &self.reach.world.pool;
            let null = matches!(pool.val(obj), nir::Val::Null)
                || matches!(pool.ty(ty), nir::Type::Null | nir::Type::Nothing);
            if !null {
                return Err("a method of a value that is no object".into());
            }
            let null = self.b.ins().iconst(ptr, 0);
            self.null_check(null)?;
            return Ok(null);
        };
        let kind = self.reach.classes.get(cls).kind;
        if kind == Kind::Module || !is_virtual(self.reach.world.pool.str(sig)) {
            let target = self.reach.classes.resolve(self.reach.world, cls, sig);
            let g = target?.ok_or_else(|| format!("no method {}", self.reach.world.pool.str(sig)))?;
            if self.is_linktime_resolved(g) {
                self.linktime_methods.insert(id, g);
            } else if kind != Kind::Module {
                let o = self.val(obj)?;
                self.null_check(o)?;
            }
            let f = self.decls.func(self.module, self.reach, g)?;
            self.methods.insert(id, f);
            let fref = self.module.declare_func_in_func(f, self.b.func);
            return Ok(self.b.ins().func_addr(ptr, fref));
        }
        let o = self.val(obj)?;
        self.null_check_of(obj, o)?;
        let rtti = self.load(ptr, o, 0);
        let slot = self.reach.classes.get(cls).slot_of.get(&sig).copied();
        if let (Some(slot), false) = (slot, kind == Kind::Trait) {
            self.reach.vcalls.insert(sig);
            return Ok(self.load(ptr, rtti, self.decls.rtti.vtable + 8 * slot as i32));
        }
        self.reach.icalls.insert(sig);
        // Temporary until #115 (AbstractMethodError): an itable miss stops the program.
        let table = self.load(ptr, rtti, self.decls.rtti.itable);
        let len = self.load(types::I64, rtti, self.decls.rtti.itable_len);
        let key = self.b.ins().iconst(types::I64, method_key(self.reach.world.pool.str(sig)) as i64);
        let lookup = self.decls.import(
            self.module,
            "scivolo_aot_itable",
            &[ptr, types::I64, types::I64],
            Some(ptr),
        )?;
        let fref = self.module.declare_func_in_func(lookup, self.b.func);
        let call = self.b.ins().call(fref, &[table, len, key]);
        Ok(self.b.inst_results(call)[0])
    }
}
