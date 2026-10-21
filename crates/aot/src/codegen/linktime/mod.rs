//! Calls of `@resolvedAtLinktime` methods, compiled to the constant they evaluate to.

mod eval;

pub use eval::{Linktime, is_linktime};

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;
use nir::{DefnKind, GlobalId, Type, Val, ValId};

impl FnBuilder<'_, '_> {
    /// The value of a call of `ptr` with `args` that is no plain call: a pair accessor, an
    /// intrinsic with an instruction, or a `@resolvedAtLinktime` method.
    pub fn special_call(&mut self, ptr: ValId, args: &[ValId]) -> R<Option<Value>> {
        if let Some(v) = self.overflow_accessor(ptr, args)? {
            return Ok(Some(v));
        }
        if let Some(v) = self.intrinsic(ptr, args)? {
            return Ok(Some(v));
        }
        let target = match *self.reach.world.pool.val(ptr) {
            Val::Global(g, _) => Some(g),
            Val::Local(l, _) => self.linktime_methods.get(&l.0).copied(),
            _ => None,
        };
        match target.filter(|&g| is_linktime(self.reach.world, g)) {
            Some(g) => self.linktime_call(g, args).map(Some),
            None => Ok(None),
        }
    }

    /// A call of `@resolvedAtLinktime` method `g`: the value its body evaluates to, else a call
    /// of its definition, which takes the arguments without the module receiver.
    fn linktime_call(&mut self, g: GlobalId, args: &[ValId]) -> R<Value> {
        if let Some(v) = self.linktime_value(g)? {
            return Ok(v);
        }
        let defn = self.reach.world.defn(g).map_err(|e| e.to_string())?;
        let DefnKind::Define { ty, .. } = defn.kind else {
            return Err("a link-time method without body".into());
        };
        let Type::Function(params, _) = self.reach.world.pool.ty(ty).clone() else {
            return Err("a link-time method of a non-function type".into());
        };
        let receiver = args.len() - params.len();
        let mut values = Vec::with_capacity(params.len());
        for (&a, p) in args[receiver..].iter().zip(params) {
            values.push(self.arg_of(a, p)?);
        }
        let f = self.decls.func(self.module, self.reach, g)?;
        let fref = self.module.declare_func_in_func(f, self.b.func);
        let call = self.b.ins().call(fref, &values);
        Ok(self.b.inst_results(call).first().copied().unwrap_or_else(|| self.b.ins().iconst(types::I8, 0)))
    }

    /// The constant a call of `@resolvedAtLinktime` method `g` returns, if its body evaluates to
    /// a boolean, an `Int` or a string.
    fn linktime_value(&mut self, g: GlobalId) -> R<Option<Value>> {
        Ok(Some(match self.decls.linktime.evaluate(self.reach.world, g)? {
            Some(Val::True) => self.b.ins().iconst(types::I8, 1),
            Some(Val::False) => self.b.ins().iconst(types::I8, 0),
            Some(Val::Int(i)) => self.b.ins().iconst(types::I32, i as u32 as i64),
            Some(Val::String(s)) => {
                let units: Vec<u16> = self.reach.world.pool.str(s).encode_utf16().collect();
                let data = self.decls.string(self.module, self.reach, &units)?;
                let gv = self.module.declare_data_in_func(data, self.b.func);
                self.b.ins().symbol_value(self.decls.ptr, gv)
            }
            _ => return Ok(None),
        }))
    }
}
