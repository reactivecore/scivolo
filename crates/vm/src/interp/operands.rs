//! Reading instruction operands and assigning block parameters.

use super::Flow;
use super::code::{Arg, Code, Target};
use super::ops::arith;
use crate::Vm;
use nir::Bin;

/// Block parameters assigned at once, kept on the stack when there are few.
const INLINE_ARGS: usize = 12;

impl Vm {
    #[inline(always)]
    pub fn arg(&mut self, code: &Code, slots: &[u64], a: Arg) -> Flow {
        match a {
            Arg::Slot(i) => Ok(slots[i as usize]),
            Arg::Imm(v) => Ok(v as u64),
            Arg::Lazy(i) => match code.lazy[i as usize].1.get() {
                0 => self.materialize(code, i),
                v => Ok(v),
            },
        }
    }

    #[inline(always)]
    pub fn pair(&mut self, code: &Code, slots: &[u64], l: Arg, r: Arg) -> Flow<(u64, u64)> {
        Ok((self.arg(code, slots, l)?, self.arg(code, slots, r)?))
    }

    /// `l bin r` for the instruction at `pc`, or a thrown `ArithmeticException` on a division
    /// by zero.
    #[inline(always)]
    pub fn binary(
        &mut self,
        code: &Code,
        slots: &[u64],
        pc: usize,
        (bin, num): (Bin, super::code::Num),
        l: Arg,
        r: Arg,
    ) -> Flow {
        let (l, r) = self.pair(code, slots, l, r)?;
        match arith::bin_total(bin, num, l, r) {
            Some(v) => Ok(v),
            None => {
                self.frames.set_pc(pc);
                self.throw_runtime("D19throwDivisionByZeronEO", &[])
            }
        }
    }

    /// Lazy operand `i` of `code`, materialized and remembered.
    #[cold]
    fn materialize(&mut self, code: &Code, i: u32) -> Flow {
        let (val, cell) = &code.lazy[i as usize];
        let v = self.val(*val, &[])?;
        cell.set(v);
        Ok(v)
    }

    /// Assigns the target block's parameters and returns its first instruction.
    #[inline(always)]
    pub fn jump(&mut self, code: &Code, slots: &mut [u64], t: &Target) -> Flow<usize> {
        match &*t.moves {
            [] => {}
            [(p, a)] => slots[*p as usize] = self.arg(code, slots, *a)?,
            [(p, a), (q, b)] => {
                let (x, y) = self.pair(code, slots, *a, *b)?;
                slots[*p as usize] = x;
                slots[*q as usize] = y;
            }
            moves => self.assign(code, slots, moves)?,
        }
        Ok(t.pc as usize)
    }

    /// Assigns block parameters from `moves` at once.
    fn assign(&mut self, code: &Code, slots: &mut [u64], moves: &[(u32, Arg)]) -> Flow<()> {
        let mut values = [0u64; INLINE_ARGS];
        let mut spill = Vec::new();
        for (i, (_, a)) in moves.iter().enumerate() {
            let v = self.arg(code, slots, *a)?;
            match i < INLINE_ARGS {
                true => values[i] = v,
                false => spill.push(v),
            }
        }
        for (i, (p, _)) in moves.iter().enumerate() {
            slots[*p as usize] = if i < INLINE_ARGS { values[i] } else { spill[i - INLINE_ARGS] };
        }
        Ok(())
    }
}
