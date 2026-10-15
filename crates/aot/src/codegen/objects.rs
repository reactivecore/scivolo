//! Operations on objects: modules, allocation, fields, method lookup and type tests.

use super::body::FnBuilder;
use crate::R;
use crate::classes::{ClassId, Kind, is_virtual, method_key};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Value, types};
use cranelift_module::Module;
use nir::{GlobalId, Op, StrId, TypeId, ValId};

impl FnBuilder<'_, '_> {
    pub fn object_op(&mut self, id: u64, op: &Op) -> R<Option<Value>> {
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
                    // Temporary until #113 aot 5 (zones).
                    return Err("allocation in a zone is not supported yet".into());
                }
                let cls = self.reach.link(*name)?;
                self.alloc(cls)?
            }
            Op::Fieldload { ty, .. } if matches!(self.reach.world.pool.ty(*ty), nir::Type::Unit) => {
                self.b.ins().iconst(types::I8, 0)
            }
            Op::Fieldload { ty, obj, name } => {
                let (o, offset) = (self.val(*obj)?, self.field_offset(*name)?);
                let t = self.ty(*ty)?;
                self.b.ins().load(t, MemFlagsData::new(), o, offset)
            }
            Op::Fieldstore { ty, obj, name, value } => {
                let (o, offset) = (self.val(*obj)?, self.field_offset(*name)?);
                let unit = matches!(self.reach.world.pool.ty(*ty), nir::Type::Unit);
                let t = if unit { self.decls.ptr } else { self.ty(*ty)? };
                let v = self.arg(*value, t)?;
                self.b.ins().store(MemFlagsData::new(), v, o, offset);
                return Ok(None);
            }
            Op::Field { obj, name } => {
                let (o, offset) = (self.val(*obj)?, self.field_offset(*name)?);
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
                let t = self.ty(*ty)?;
                let at = self.element(*ty, *arr, *idx)?;
                let v = self.arg(*value, t)?;
                self.b.ins().store(MemFlagsData::new(), v, at, 0);
                return Ok(None);
            }
            Op::Arraylength(arr) => self.array_length(*arr)?,
            Op::Box { ty, obj } => self.boxing(*ty, *obj, true)?,
            Op::Unbox { ty, obj } => self.boxing(*ty, *obj, false)?,
            // Temporary until #113 aot 3 (slow paths): a failing cast throws ClassCastException.
            Op::As { obj, .. } => self.val(*obj)?,
            // Temporary until #113 aot 3 (heap) and aot 5 (stackalloc, externs).
            op => return Err(format!("operation not supported yet: {op:?}")),
        }))
    }

    /// A new instance of `cls` from the collector, with its header set.
    pub fn alloc(&mut self, cls: ClassId) -> R<Value> {
        self.reach.allocate(cls);
        let ptr = self.decls.ptr;
        let alloc = self.decls.import(self.module, "scalanative_GC_alloc", &[ptr, ptr], Some(ptr))?;
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

    fn field_offset(&mut self, name: GlobalId) -> R<i32> {
        let owner = self.reach.link(self.reach.world.owner(name))?;
        match self.reach.classes.get(owner).fields.get(&name) {
            Some(f) => Ok(f.offset as i32),
            None => Err(format!("no field {}", self.reach.world.name(name))),
        }
    }

    fn load(&mut self, t: types::Type, addr: Value, offset: i32) -> Value {
        self.b.ins().load(t, MemFlagsData::new(), addr, offset)
    }

    /// The address of method `sig` of `obj`: resolved here for modules and non-virtual methods,
    /// else from the vtable of its class or, if its static class has no slot, from the itable.
    fn method(&mut self, id: u64, obj: ValId, sig: StrId) -> R<Value> {
        let ty = self.type_of(obj)?;
        let cls = self.reach.class_of_type(ty)?.ok_or("a method of a value that is no object")?;
        let kind = self.reach.classes.get(cls).kind;
        let ptr = self.decls.ptr;
        if kind == Kind::Module || !is_virtual(self.reach.world.pool.str(sig)) {
            let target = self.reach.classes.resolve(self.reach.world, cls, sig);
            let g = target.ok_or_else(|| format!("no method {}", self.reach.world.pool.str(sig)))?;
            let f = self.decls.func(self.module, self.reach, g)?;
            self.methods.insert(id, f);
            let fref = self.module.declare_func_in_func(f, self.b.func);
            return Ok(self.b.ins().func_addr(ptr, fref));
        }
        let o = self.val(obj)?;
        // Temporary until #113 aot 3 (slow paths): a null receiver throws NullPointerException.
        let rtti = self.load(ptr, o, 0);
        let slot = self.reach.classes.get(cls).slot_of.get(&sig).copied();
        if let (Some(slot), false) = (slot, kind == Kind::Trait) {
            self.reach.vcalls.insert(sig);
            return Ok(self.load(ptr, rtti, self.decls.rtti.vtable + 8 * slot as i32));
        }
        self.reach.icalls.insert(sig);
        // Temporary until #113 aot 3 (slow paths): an itable miss throws AbstractMethodError
        // instead of returning null.
        let table = self.load(ptr, rtti, self.decls.rtti.itable);
        let len = self.load(types::I64, rtti, self.decls.rtti.itable_len);
        let key = self.b.ins().iconst(types::I64, method_key(self.reach.world.pool.str(sig)) as i64);
        let lookup =
            self.decls.import(self.module, "scalai_aot_itable", &[ptr, types::I64, types::I64], Some(ptr))?;
        let fref = self.module.declare_func_in_func(lookup, self.b.func);
        let call = self.b.ins().call(fref, &[table, len, key]);
        Ok(self.b.inst_results(call)[0])
    }

    /// Whether `obj` is an instance of type `ty`: a class id range test or a search of the
    /// object's traits.
    fn is_instance(&mut self, obj: Value, ty: TypeId) -> R<Value> {
        let cls = self.reach.class_of_type(ty)?.ok_or("a type test against a non-class type")?;
        let ptr = self.decls.ptr;
        let test = self.b.create_block();
        let done = self.b.create_block();
        self.b.append_block_param(done, types::I8);
        let no = self.b.ins().iconst(types::I8, 0);
        self.b.ins().brif(obj, test, &[], done, &[no.into()]);
        self.b.switch_to_block(test);
        let target = self.rtti_addr(cls)?;
        let rtti = self.load(ptr, obj, 0);
        let l = &self.decls.rtti;
        let (id, range, interfaces, count) = (l.id, l.range, l.interfaces, l.interfaces_count);
        let result = if self.reach.classes.get(cls).kind == Kind::Trait {
            let list = self.load(ptr, rtti, interfaces);
            let n = self.load(types::I32, rtti, count);
            let has = self.decls.import(
                self.module,
                "scalai_aot_has_trait",
                &[ptr, types::I32, ptr],
                Some(types::I8),
            )?;
            let fref = self.module.declare_func_in_func(has, self.b.func);
            let call = self.b.ins().call(fref, &[list, n, target]);
            self.b.inst_results(call)[0]
        } else {
            let own = self.load(types::I32, rtti, id);
            let lo = self.load(types::I32, target, id);
            let hi = self.load(types::I32, target, range);
            let offset = self.b.ins().isub(own, lo);
            let width = self.b.ins().isub(hi, lo);
            self.b.ins().icmp(IntCC::UnsignedLessThanOrEqual, offset, width)
        };
        self.b.ins().jump(done, &[result.into()]);
        self.b.switch_to_block(done);
        Ok(self.b.block_params(done)[0])
    }
}
