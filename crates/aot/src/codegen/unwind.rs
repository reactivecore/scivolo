//! Exceptions as a flag (ADR 0028): a thrown object waits in `scivolo_aot_exception`; after
//! each call the caller jumps to its handler or returns at once, leaving the slot set.

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::{Block, InstBuilder, MemFlagsData, Value, types};
use cranelift_module::Module;
use nir::{Next, Op, Type, TypeId, ValId};

/// Whether `op` calls code that may throw.
pub fn may_throw(op: &Op) -> bool {
    matches!(op, Op::Call { .. } | Op::Module(_) | Op::Box { .. } | Op::Unbox { .. })
}

impl FnBuilder<'_, '_> {
    fn exception_slot(&mut self) -> Value {
        let gv = self.module.declare_data_in_func(self.decls.exception, self.b.func);
        self.b.ins().symbol_value(self.decls.ptr, gv)
    }

    /// After a call: to `unwind`'s handler with the thrown object, or out of this function, when
    /// the call threw.
    pub fn after_call(&mut self, unwind: &Next) -> R<()> {
        let slot = self.exception_slot();
        let exc = self.b.ins().load(self.decls.ptr, MemFlagsData::new(), slot, 0);
        let cont = self.b.create_block();
        match unwind {
            Next::Unwind(param, to) => {
                let handler = self.b.create_block();
                self.b.ins().brif(exc, handler, &[], cont, &[]);
                self.b.switch_to_block(handler);
                let zero = self.b.ins().iconst(self.decls.ptr, 0);
                self.b.ins().store(MemFlagsData::new(), zero, slot, 0);
                self.vals.insert(param.id.0, exc);
                self.jump(to)?;
            }
            _ => {
                let out = self.propagate_block();
                self.b.ins().brif(exc, out, &[], cont, &[]);
            }
        }
        self.b.switch_to_block(cont);
        Ok(())
    }

    /// `throw v`: to `unwind`'s handler, or out of this function with the slot set; `throw null`
    /// throws a NullPointerException, a value that is no Throwable a ClassCastException.
    pub fn throw(&mut self, v: ValId, unwind: &Next) -> R<()> {
        self.unwind = unwind.clone();
        let exc = self.val(v)?;
        self.null_check_of(v, exc)?;
        let world = &mut self.reach.world;
        let name = world.top("java.lang.Throwable");
        let throwable = TypeId(world.pool.types.intern(Type::Ref { name, exact: false, nullable: true }));
        let exc = self.cast(v, throwable)?;
        if let Next::Unwind(param, to) = unwind {
            self.vals.insert(param.id.0, exc);
            return self.jump(to);
        }
        let slot = self.exception_slot();
        self.b.ins().store(MemFlagsData::new(), exc, slot, 0);
        let out = self.propagate_block();
        self.b.ins().jump(out, &[]);
        Ok(())
    }

    fn propagate_block(&mut self) -> Block {
        *self.propagate.get_or_insert_with(|| self.b.create_block())
    }

    /// The block returning zero values to a caller that finds the slot set.
    pub fn define_propagate(&mut self) {
        let Some(block) = self.propagate else { return };
        self.b.switch_to_block(block);
        let types: Vec<types::Type> = self.b.func.signature.returns.iter().map(|r| r.value_type).collect();
        let zeros: Vec<Value> = types
            .into_iter()
            .map(|t| match t {
                types::F32 => self.b.ins().f32const(0.0),
                types::F64 => self.b.ins().f64const(0.0),
                t => self.b.ins().iconst(t, 0),
            })
            .collect();
        self.b.ins().return_(&zeros);
    }
}
