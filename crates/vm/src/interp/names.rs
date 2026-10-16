//! Names and source positions of interpreted methods, for traces and stack traces.

use super::code::Code;
use super::{Func, FuncId};
use crate::Vm;
use nir::{GlobalId, PosId};

impl Vm {
    /// The names of the innermost `depth` interpreted methods on the call stack, which is only
    /// recorded while tracing.
    pub fn backtrace(&self, depth: usize) -> Vec<String> {
        self.funcs.stack.iter().rev().take(depth).map(|&f| self.func_name(f)).collect()
    }

    /// The source file and zero-based line of instruction `pc` of the code of tier `tier` of
    /// method `g`, once lowered.
    pub fn source_position(&self, g: GlobalId, pc: usize, tier: u8) -> Option<(&str, u32)> {
        Some(self.source_of(self.position_at(g, pc, tier)?))
    }

    /// The position of instruction `pc` of the code of tier `tier` of method `g`.
    pub fn position_at(&self, g: GlobalId, pc: usize, tier: u8) -> Option<PosId> {
        self.instruction(g, pc, tier).map(|(_, _, pos)| pos)
    }

    /// The methods executing instruction `pc` of the code of tier `tier` of method `g` with
    /// the position each is at, innermost first, if it was inlined from another method.
    pub fn inline_chain(&self, g: GlobalId, pc: usize, tier: u8) -> Option<Vec<(GlobalId, PosId)>> {
        let (code, inst, pos) = self.instruction(g, pc, tier)?;
        code.inlined.as_ref()?.chain(g, inst, pos)
    }

    /// The source file and zero-based line of position `pos`.
    pub fn source_of(&self, pos: PosId) -> (&str, u32) {
        let pos = self.world.pool.pos(pos);
        (self.world.pool.str(pos.file), pos.line)
    }

    /// The code, NIR instruction index and position of instruction `pc` of tier `tier` of `g`.
    fn instruction(&self, g: GlobalId, pc: usize, tier: u8) -> Option<(&Code, usize, PosId)> {
        let f = *self.funcs.by_name.get(&g)?;
        let Func::Nir { code, retired, .. } = &self.funcs.all[f as usize] else { return None };
        let code = [code, retired].into_iter().flatten().find(|c| c.tier == tier)?;
        let nir::DefnKind::Define { body, .. } = &code.defn.kind else { return None };
        let inst = *code.origin.get(pc)? as usize;
        Some((code, inst, *body.positions.get(inst)?))
    }

    pub fn func_name(&self, f: FuncId) -> String {
        match &self.funcs.all[f as usize] {
            Func::Nir { name, .. } | Func::Native { name, .. } | Func::Proxy { name, .. } => {
                self.world.name(*name)
            }
        }
    }
}
