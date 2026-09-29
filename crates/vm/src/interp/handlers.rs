//! Instructions that may throw, run outside `run`'s fast arms, and where what they throw lands.

use super::code::{Code, Instr, Target};
use super::exec::body;
use super::{Flow, Unwind};
use crate::Vm;
use nir::Inst;

impl Vm {
    /// Runs value instruction `instr` at `pc`: `None` to continue with the next instruction, else
    /// the handler it throws to.
    pub fn compute_at<'c>(
        &mut self,
        code: &Code,
        slots: &mut [u64],
        pc: usize,
        instr: &'c Instr,
    ) -> Flow<Option<&'c Target>> {
        self.frames.set_pc(pc);
        match self.compute(code, slots, instr) {
            Ok(()) => Ok(None),
            Err(u) => self.unwind_to(code, slots, pc, instr, u).map(Some),
        }
    }

    /// Where `instr` at `pc` continues after failing with `u`: its handler, with the thrown
    /// exception in the handler's slot, or `u` again when it has none.
    #[cold]
    pub fn unwind_to<'c>(
        &mut self,
        code: &Code,
        slots: &mut [u64],
        pc: usize,
        instr: &'c Instr,
        u: Unwind,
    ) -> Flow<&'c Target> {
        let u = self.as_linkage_error(u);
        if self.funcs.trace && u.thrown().is_some() {
            self.trace_throw(code, pc);
        }
        let (Some(exc), Some(h)) = (u.thrown(), instr.unwind()) else { return Err(u) };
        slots[h.0 as usize] = exc as u64;
        Ok(&h.1)
    }

    fn trace_throw(&self, code: &Code, pc: usize) {
        if let Some(Inst::Let { op, .. }) = body(code).0.get(code.origin[pc] as usize) {
            eprintln!("  thrown by {}", nir::show::op(&self.world.pool, op));
        }
    }
}
