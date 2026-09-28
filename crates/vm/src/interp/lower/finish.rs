//! Resolving aliases, and renumbering the NIR operations lowering keeps to slots.

use crate::interp::code::Instr;
use nir::{Local, Pool, Type, Val, ValId};
use rustc_hash::FxHashMap;

/// The value `v` names, through aliases.
pub fn resolve(pool: &Pool, aliases: &FxHashMap<u64, ValId>, mut v: ValId) -> ValId {
    for _ in 0..=aliases.len() {
        match pool.val(v) {
            Val::Local(l, _) => match aliases.get(&l.0) {
                Some(&a) => v = a,
                None => return v,
            },
            _ => return v,
        }
    }
    unreachable!("aliases form a cycle")
}

/// Rewrites the operands of the NIR operations in `instrs` to slot numbers.
pub fn renumber_ops(
    instrs: &mut [Instr],
    pool: &mut Pool,
    slots: &mut FxHashMap<u64, u32>,
    aliases: &FxHashMap<u64, ValId>,
    constants: &FxHashMap<u64, u64>,
) {
    let mut r = Renumber { pool, slots, aliases, constants };
    for instr in instrs {
        if let Instr::Op { op, .. } = instr {
            **op = op.map_operands(&mut |v| r.val(v));
        }
    }
}

/// The literal of type `ty` whose bits `immediate` gives as `bits`; `Size` for other types.
fn literal(ty: &Type, bits: u64) -> Val {
    match ty {
        Type::Bool => {
            if bits & 1 == 1 {
                Val::True
            } else {
                Val::False
            }
        }
        Type::Byte => Val::Byte(bits as i8),
        Type::Short => Val::Short(bits as i16),
        Type::Char => Val::Char(bits as u16),
        Type::Int => Val::Int(bits as i32),
        Type::Long => Val::Long(bits as i64),
        Type::Float => Val::Float(bits as u32),
        Type::Double => Val::Double(bits),
        _ => Val::Size(bits),
    }
}

struct Renumber<'a> {
    pool: &'a mut Pool,
    slots: &'a mut FxHashMap<u64, u32>,
    aliases: &'a FxHashMap<u64, ValId>,
    constants: &'a FxHashMap<u64, u64>,
}

impl Renumber<'_> {
    /// `v` with aliases resolved and each local, also inside aggregates, replaced by its slot
    /// number or, if its value is known, a literal of its type.
    fn val(&mut self, v: ValId) -> ValId {
        let v = resolve(self.pool, self.aliases, v);
        let renamed = match self.pool.val(v).clone() {
            Val::Local(l, ty) => match self.constants.get(&l.0) {
                Some(&bits) => literal(self.pool.ty(ty), bits),
                None => {
                    let next = self.slots.len() as u32;
                    let slot = *self.slots.entry(l.0).or_insert(next);
                    Val::Local(Local(slot as u64), ty)
                }
            },
            Val::StructValue(vs) => Val::StructValue(vs.iter().map(|v| self.val(*v)).collect()),
            Val::ArrayValue(ty, vs) => Val::ArrayValue(ty, vs.iter().map(|v| self.val(*v)).collect()),
            Val::Const(inner) => Val::Const(self.val(inner)),
            _ => return v,
        };
        ValId(self.pool.vals.intern(renamed))
    }
}
