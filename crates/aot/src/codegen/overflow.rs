//! `llvm.{s,u}{add,sub,mul}.with.overflow` intrinsics as Cranelift instructions: their result
//! is a pair (value, overflowed) that `extract` takes apart.

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::{InstBuilder, Value};
use nir::{Val, ValId};

impl FnBuilder<'_, '_> {
    /// Computes the intrinsic `ptr` names into the pair of local `id`, if it is one of these.
    pub fn with_overflow(&mut self, id: u64, ptr: ValId, args: &[ValId]) -> R<bool> {
        let pool = &self.reach.world.pool;
        let Val::Global(g, _) = *pool.val(ptr) else { return Ok(false) };
        let Some(sig) = pool.member_sig(g) else { return Ok(false) };
        let Some(name) = sig.strip_prefix('C').map(|s| s.trim_start_matches(|c: char| c.is_ascii_digit()))
        else {
            return Ok(false);
        };
        let Some(op) = name.strip_prefix("llvm.").and_then(|n| n.split(".with.overflow.").next()) else {
            return Ok(false);
        };
        if !name.contains(".with.overflow.") || args.len() != 2 {
            return Ok(false);
        }
        let op = op.to_owned();
        let (l, r) = (self.val(args[0])?, self.val(args[1])?);
        let ins = self.b.ins();
        let (value, overflowed) = match op.as_str() {
            "sadd" => ins.sadd_overflow(l, r),
            "uadd" => ins.uadd_overflow(l, r),
            "ssub" => ins.ssub_overflow(l, r),
            "usub" => ins.usub_overflow(l, r),
            "smul" => ins.smul_overflow(l, r),
            "umul" => ins.umul_overflow(l, r),
            other => return Err(format!("intrinsic {other}.with.overflow")),
        };
        self.pairs.insert(id, vec![value, overflowed]);
        Ok(true)
    }

    /// A call of the accessor `value` or `flag` of `LLVMIntrinsics.IntOverflow`/`LongOverflow`
    /// on a pair: its part.
    pub fn overflow_accessor(&mut self, ptr: ValId, args: &[ValId]) -> R<Option<Value>> {
        let pool = &self.reach.world.pool;
        let Val::Global(g, _) = *pool.val(ptr) else { return Ok(None) };
        let owner = pool.top_name(g);
        let overflow =
            owner.ends_with("LLVMIntrinsics$IntOverflow") || owner.ends_with("LLVMIntrinsics$LongOverflow");
        let index = match pool.member_sig(g) {
            Some(s) if overflow && s.starts_with("D5value") => 0,
            Some(s) if overflow && s.starts_with("D4flag") => 1,
            _ => return Ok(None),
        };
        let [pair] = args else { return Ok(None) };
        self.extract(*pair, &[index]).map(Some)
    }

    /// `extract aggr, indexes` of a pair an intrinsic computed.
    pub fn extract(&mut self, aggr: ValId, indexes: &[i32]) -> R<Value> {
        let Val::Local(l, _) = *self.reach.world.pool.val(aggr) else {
            return Err("extract from a constant".into());
        };
        let parts = self.pairs.get(&l.0).ok_or("extract from an aggregate value")?;
        let [i] = indexes else { return Err("a nested extract".into()) };
        parts.get(*i as usize).copied().ok_or_else(|| format!("extract index {i}"))
    }
}
