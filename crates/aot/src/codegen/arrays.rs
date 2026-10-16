//! Arrays and boxes: allocation, element access and length; boxing through the runtime's
//! `BoxesRunTime` and `Boxes` modules.

use super::body::FnBuilder;
use crate::R;
use crate::classes::ClassId;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Value, types};
use cranelift_module::Module;
use nir::{Type, TypeId, Val, ValId};
use vm::{ARRAY_HEADER, ARRAY_LENGTH, size_of};

impl FnBuilder<'_, '_> {
    /// A new array of `elem`s, of length `init` or holding the elements of constant `init`.
    pub fn array_alloc(&mut self, elem: TypeId, init: ValId) -> R<Value> {
        let ptr = self.decls.ptr;
        let cls = self.reach.array_class(elem)?;
        self.reach.allocate(cls);
        let stride = size_of(&self.reach.world.pool, elem);
        let elems = match self.reach.world.pool.val(init) {
            Val::ArrayValue(_, elems) => Some(elems.clone()),
            _ => None,
        };
        let len = match &elems {
            Some(elems) => self.b.ins().iconst(ptr, elems.len() as i64),
            None => {
                let n = self.val(init)?;
                self.length_check(cls, n)?;
                self.b.ins().sextend(ptr, n)
            }
        };
        let alloc = self.decls.import(self.module, "scalai_aot_alloc_array", &[ptr, ptr, ptr], Some(ptr))?;
        let rtti = self.rtti_addr(cls)?;
        let stride_v = self.b.ins().iconst(ptr, stride as i64);
        let fref = self.module.declare_func_in_func(alloc, self.b.func);
        let call = self.b.ins().call(fref, &[rtti, len, stride_v]);
        let arr = self.b.inst_results(call)[0];
        for (i, &e) in elems.iter().flatten().enumerate() {
            let v = self.arg_of(e, elem)?;
            let at = ARRAY_HEADER as i32 + (stride as usize * i) as i32;
            self.b.ins().store(MemFlagsData::new(), v, arr, at);
        }
        Ok(arr)
    }

    /// Throws NegativeArraySizeException through the array class's `alloc` unless `0 <= n`.
    fn length_check(&mut self, cls: ClassId, n: Value) -> R<()> {
        let name = self.reach.classes.get(cls).name;
        let class = self.reach.world.pool.top_name(name).to_owned();
        let g = self.reach.world.member(name, &format!("D5allociL{}{class}Eo", class.len()));
        let f = self.decls.func(self.module, self.reach, g)?;
        let ok = self.b.ins().icmp_imm_s(IntCC::SignedGreaterThanOrEqual, n, 0);
        self.check_call(ok, f, &[n])
    }

    /// The address of element `idx` of array `arr` of `elem`s, checked against null and bounds.
    pub fn element(&mut self, elem: TypeId, arr: ValId, idx: ValId) -> R<Value> {
        let (a, i) = (self.val(arr)?, self.val(idx)?);
        self.null_check_of(arr, a)?;
        let len = self.b.ins().load(types::I32, MemFlagsData::new(), a, ARRAY_LENGTH as i32);
        self.bounds_check(i, len)?;
        let i = self.b.ins().sextend(self.decls.ptr, i);
        let offset = self.b.ins().imul_imm_s(i, size_of(&self.reach.world.pool, elem) as i64);
        let base = self.b.ins().iadd(a, offset);
        Ok(self.b.ins().iadd_imm_s(base, ARRAY_HEADER as i64))
    }

    pub fn array_length(&mut self, arr: ValId) -> R<Value> {
        let a = self.val(arr)?;
        self.null_check_of(arr, a)?;
        Ok(self.b.ins().load(types::I32, MemFlagsData::new(), a, ARRAY_LENGTH as i32))
    }

    /// `obj` boxed into class type `ty` (`boxing`) or unboxed from it, through the runtime.
    pub fn boxing(&mut self, ty: TypeId, obj: ValId, boxing: bool) -> R<Value> {
        let Type::Ref { name, .. } = *self.reach.world.pool.ty(ty) else {
            return Err("box into a non-class type".into());
        };
        let class = self.reach.world.pool.top_name(name).to_owned();
        let (owner, sig) = vm::box_method(&class, boxing).ok_or_else(|| format!("box {class}"))?;
        let owner = self.reach.world.top(owner);
        let g = self.reach.world.member(owner, &sig);
        let cls = self.reach.link(owner)?;
        let load = self.decls.module_load(self.module, self.reach, cls)?;
        let f = self.decls.func(self.module, self.reach, g)?;
        let load = self.module.declare_func_in_func(load, self.b.func);
        let call = self.b.ins().call(load, &[]);
        let instance = self.b.inst_results(call)[0];
        let param = self.module.declarations().get_function_decl(f).signature.params[1].value_type;
        let v = self.arg(obj, param)?;
        let fref = self.module.declare_func_in_func(f, self.b.func);
        let call = self.b.ins().call(fref, &[instance, v]);
        Ok(self.b.inst_results(call)[0])
    }
}
