//! Locals whose value is known when lowering: constructed modules and conversions of constants.

use super::{immediate, num, val_num};
use crate::interp::ops::arith;
use nir::{Body, GlobalId, Inst, Op, Pool};
use rustc_hash::FxHashMap;

/// The value of each such local, by local; `constructed` gives the address of a module whose
/// construction has finished.
pub fn constants(
    body: &Body,
    pool: &Pool,
    constructed: &dyn Fn(GlobalId) -> Option<u64>,
) -> FxHashMap<u64, u64> {
    body.insts
        .iter()
        .filter_map(|inst| match inst {
            Inst::Let { id, op: Op::Module(name), .. } => constructed(*name).map(|obj| (id.0, obj)),
            Inst::Let { id, op: Op::Conv { conv, ty, value }, .. } => immediate(pool.val(*value))
                .map(|bits| (id.0, arith::conv(*conv, val_num(pool, *value), num(pool.ty(*ty)), bits))),
            _ => None,
        })
        .collect()
}
