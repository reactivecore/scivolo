//! `llvm.*` intrinsics Cranelift has instructions for; the others call C (`c/natives.c`).

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::{InstBuilder, Value};
use nir::{Val, ValId};

impl FnBuilder<'_, '_> {
    /// The value of a call of `ptr` with `args` if `ptr` is an intrinsic with an instruction.
    pub fn intrinsic(&mut self, ptr: ValId, args: &[ValId]) -> R<Option<Value>> {
        let pool = &self.reach.world.pool;
        let Val::Global(g, _) = *pool.val(ptr) else { return Ok(None) };
        let Some(name) = pool.member_sig(g).and_then(nir::demangle::extern_name) else { return Ok(None) };
        let Some(op) = name.strip_prefix("llvm.").and_then(|n| n.split('.').next()) else { return Ok(None) };
        let op = op.to_owned();
        let Some(&first) = args.first() else { return Ok(None) };
        let x = self.val(first)?;
        let ins = self.b.ins();
        Ok(Some(match op.as_str() {
            "ceil" => ins.ceil(x),
            "floor" => ins.floor(x),
            "sqrt" => ins.sqrt(x),
            "fabs" => ins.fabs(x),
            "trunc" => ins.trunc(x),
            "rint" | "nearbyint" => ins.nearest(x),
            "ctlz" => ins.clz(x),
            "cttz" => ins.ctz(x),
            "ctpop" => ins.popcnt(x),
            "bswap" => ins.bswap(x),
            "copysign" if args.len() == 2 => {
                let y = self.val(args[1])?;
                self.b.ins().fcopysign(x, y)
            }
            _ => return Ok(None),
        }))
    }
}
