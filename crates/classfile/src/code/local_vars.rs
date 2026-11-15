// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! The `LocalVariableTable`: the named locals and the ranges of code where they hold a value,
//! those of a block from their definition to its end (dotty's `varsInScope`,
//! `emitLocalVarScopes`).

use super::{Code, Label};
use crate::pool::Pool;

/// A local: its name, descriptor, range of code and slot.
struct LocalVar {
    name: String,
    descriptor: String,
    start: Label,
    end: Label,
    slot: u16,
}

/// The locals of the table, and those of the open blocks, which end with their block.
#[derive(Default)]
pub struct LocalVars {
    table: Vec<LocalVar>,
    open: Vec<(String, String, Label, u16)>,
    /// For each open block, the number of locals of the blocks around it.
    blocks: Vec<usize>,
}

impl Code {
    /// Names local `slot` of `descriptor` from label `start` to `end` in the
    /// `LocalVariableTable`, after the locals named before.
    pub fn local_var(&mut self, name: &str, descriptor: &str, (start, end): (Label, Label), slot: u16) {
        let (name, descriptor) = (name.to_owned(), descriptor.to_owned());
        self.local_vars.table.push(LocalVar { name, descriptor, start, end, slot });
    }

    /// Opens a block, whose locals end where it is closed.
    pub fn open_block(&mut self) {
        self.local_vars.blocks.push(self.local_vars.open.len());
    }

    /// Names local `slot` of `descriptor` of the innermost open block from here to the block's end.
    pub fn block_local(&mut self, name: &str, descriptor: &str, slot: u16) {
        let start = self.mark();
        self.local_vars.open.push((name.to_owned(), descriptor.to_owned(), start, slot));
    }

    /// Closes the innermost open block: names its locals, in the order of their definitions.
    pub fn close_block(&mut self) {
        let Some(from) = self.local_vars.blocks.pop() else { return };
        let end = self.mark();
        let open: Vec<_> = self.local_vars.open.drain(from..).collect();
        for (name, descriptor, start, slot) in open {
            self.local_vars.table.push(LocalVar { name, descriptor, start, end, slot });
        }
    }

    /// Adds each local's name and descriptor to `pool`; their indices.
    pub fn local_var_names(&self, pool: &mut Pool) -> Vec<[u16; 2]> {
        self.local_vars.table.iter().map(|v| [pool.utf8(&v.name), pool.utf8(&v.descriptor)]).collect()
    }

    /// Whether a local's range ends at a label never placed.
    pub fn local_var_unplaced(&self) -> bool {
        self.local_vars.table.iter().any(|v| self.offset(v.start).is_none() || self.offset(v.end).is_none())
    }

    /// The body of the `LocalVariableTable` with the indices `names` of `local_var_names`, none
    /// without locals.
    pub fn local_var_table(&self, names: &[[u16; 2]]) -> Option<Vec<u8>> {
        let table = &self.local_vars.table;
        if table.is_empty() {
            return None;
        }
        let mut out = (table.len() as u16).to_be_bytes().to_vec();
        for (v, [name, descriptor]) in table.iter().zip(names) {
            let start = self.offset(v.start).unwrap_or(0);
            let end = self.offset(v.end).unwrap_or(start);
            for x in [start as u16, (end - start) as u16, *name, *descriptor, v.slot] {
                out.extend(x.to_be_bytes());
            }
        }
        Some(out)
    }
}
