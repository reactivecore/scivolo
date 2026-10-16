//! The stack trace element of a frame, from the code-address table, as the VM's native
//! `StackTrace.withNameFromUnwind` makes it.

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, StackSlotData, StackSlotKind, Value, types};
use cranelift_module::Module;

const ELEMENT: &str = "java.lang.StackTraceElement";
const INIT: &str = "RL16java.lang.StringL16java.lang.StringL16java.lang.StringiE";

impl FnBuilder<'_, '_> {
    /// A new `StackTraceElement` of the call at return address `ip`, as `scalai_aot_frame_info`
    /// describes it.
    pub fn trace_element(&mut self, ip: Value) -> R<Value> {
        let ptr = self.decls.ptr;
        let slot = self.b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 32, 3));
        let out = self.b.ins().stack_addr(ptr, slot, 0);
        let info = self.decls.import(self.module, "scalai_aot_frame_info", &[ptr, ptr], None)?;
        let info = self.module.declare_func_in_func(info, self.b.func);
        self.b.ins().call(info, &[ip, out]);
        let [class, method, file] = [0, 8, 16].map(|at| self.b.ins().load(ptr, MemFlagsData::new(), out, at));
        let line = self.b.ins().load(types::I32, MemFlagsData::new(), out, 24);
        let cls = self.reach.link_top(ELEMENT)?;
        let obj = self.alloc(cls)?;
        let init = self.reach.world.member(self.reach.classes.get(cls).name, INIT);
        let init = self.decls.func(self.module, self.reach, init)?;
        let init = self.module.declare_func_in_func(init, self.b.func);
        self.b.ins().call(init, &[obj, class, method, file, line]);
        Ok(obj)
    }
}
