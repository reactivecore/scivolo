//! Type tests: `is` by class id range or trait search, and the checked `as`.

use super::body::FnBuilder;
use crate::R;
use crate::classes::Kind;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;
use nir::{TypeId, ValId};

impl FnBuilder<'_, '_> {
    /// `obj as ty`: `obj` if it is null or an instance of `ty`, else a thrown ClassCastException;
    /// unchecked when the static class of `obj` is a subclass of `ty`.
    pub fn cast(&mut self, obj: ValId, ty: TypeId) -> R<Value> {
        let o = self.val(obj)?;
        let Some(cls) = self.reach.class_of_type(ty)? else { return Ok(o) };
        let from = self.class_of_val(obj).ok().flatten();
        if from.is_some_and(|f| self.reach.classes.get(f).ancestors.contains(&cls)) {
            return Ok(o);
        }
        let is = self.is_instance(o, ty)?;
        let null = self.b.ins().icmp_imm_s(IntCC::Equal, o, 0);
        let ok = self.b.ins().bor(is, null);
        let target = self.rtti_addr(cls)?;
        let readable = self.b.ins().select(null, target, o);
        let from = self.load(self.decls.ptr, readable, 0);
        self.check(ok, super::checks::CLASS_CAST, &[from, target])?;
        Ok(o)
    }

    /// Whether `obj` is an instance of type `ty`: a class id range test or a search of the
    /// object's traits.
    pub fn is_instance(&mut self, obj: Value, ty: TypeId) -> R<Value> {
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
        let (id, range, traits, count) = (l.id, l.range, l.traits, l.traits_len);
        let result = if self.reach.classes.get(cls).kind == Kind::Trait {
            let list = self.load(ptr, rtti, traits);
            let n = self.load(types::I32, rtti, count);
            let params = [ptr, types::I32, ptr];
            let has = self.decls.import(self.module, "scalai_aot_has_trait", &params, Some(types::I8))?;
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
