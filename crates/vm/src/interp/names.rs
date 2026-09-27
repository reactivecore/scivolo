//! Names and source positions of interpreted methods, for traces and stack traces.

use super::{Func, FuncId};
use crate::Vm;
use nir::GlobalId;

impl Vm {
    /// The names of the innermost `depth` interpreted methods on the call stack, which is only
    /// recorded while tracing.
    pub fn backtrace(&self, depth: usize) -> Vec<String> {
        self.funcs.stack.iter().rev().take(depth).map(|&f| self.func_name(f)).collect()
    }

    /// The source file and zero-based line of instruction `pc` of the code of tier `tier` of
    /// method `g`, once lowered.
    pub fn source_position(&self, g: GlobalId, pc: usize, tier: u8) -> Option<(&str, u32)> {
        let f = *self.funcs.by_name.get(&g)?;
        let Func::Nir { code, retired, .. } = &self.funcs.all[f as usize] else { return None };
        let code = [code, retired].into_iter().flatten().find(|c| c.tier == tier)?;
        let nir::DefnKind::Define { body, .. } = &code.defn.kind else { return None };
        let inst = *code.origin.get(pc)? as usize;
        let pos = self.world.pool.pos(*body.positions.get(inst)?);
        Some((self.world.pool.str(pos.file), pos.line))
    }

    pub fn func_name(&self, f: FuncId) -> String {
        match &self.funcs.all[f as usize] {
            Func::Nir { name, .. } | Func::Native { name, .. } | Func::Proxy { name, .. } => {
                self.world.name(*name)
            }
        }
    }
}
