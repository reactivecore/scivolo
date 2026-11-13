//! Jumps whose offset does not fit in two bytes (ASM's `EXPAND_ASM_INSNS`): `goto` becomes
//! `goto_w`, a conditional jump its inverse over a `goto_w`.

use super::Code;
use super::jumps::Fixup;
use crate::frames::length;
use crate::opcodes::{GOTO, GOTO_W};
use std::collections::BTreeSet;

impl Code {
    /// Rewrites the jumps that do not reach and moves the offsets after them, until all reach.
    pub fn widen(&mut self) {
        while !self.far.is_empty() {
            let far: BTreeSet<usize> = std::mem::take(&mut self.far).into_iter().collect();
            let moved = self.relayout(&far);
            for l in self.labels.iter_mut().flatten() {
                *l = moved[*l];
            }
            for line in &mut self.lines {
                line.0 = moved[usize::from(line.0)] as u16;
            }
            let jumps = std::mem::take(&mut self.jumps);
            self.jumps = jumps.iter().map(|f| moved_jump(f, &far, &moved, &self.bytes)).collect();
            for f in self.jumps.clone() {
                let to = self.offset(f.label).expect("a placed label");
                self.patch(&f, to);
            }
        }
    }

    /// The code with the jumps at `far` widened and switches padded anew; the new offset of each
    /// old instruction and of the end.
    fn relayout(&mut self, far: &BTreeSet<usize>) -> Vec<usize> {
        let old = std::mem::take(&mut self.bytes);
        let mut moved = vec![0; old.len() + 1];
        let mut at = 0;
        while at < old.len() {
            let (op, len, here) = (old[at], length(&old, at), self.bytes.len());
            moved[at] = here;
            match op {
                GOTO if far.contains(&at) => self.bytes.extend([GOTO_W, 0, 0, 0, 0]),
                _ if far.contains(&at) => self.bytes.extend([inverse(op), 0, 8, GOTO_W, 0, 0, 0, 0]),
                0xaa | 0xab => {
                    self.bytes.push(op);
                    self.bytes.resize(aligned(here), 0);
                    self.bytes.extend(&old[aligned(at)..at + len]);
                }
                _ => self.bytes.extend(&old[at..at + len]),
            }
            at += len;
        }
        moved[old.len()] = self.bytes.len();
        moved
    }
}

/// `f` after the relayout `moved` into `bytes` widening the jumps at `far`.
fn moved_jump(f: &Fixup, far: &BTreeSet<usize>, moved: &[usize], bytes: &[u8]) -> Fixup {
    let insn = moved[f.insn];
    match bytes[insn] {
        0xaa | 0xab => Fixup { insn, operand: f.operand - aligned(f.insn) + aligned(insn), ..*f },
        GOTO_W => Fixup { insn, operand: insn + 1, label: f.label, wide: true },
        _ if far.contains(&f.insn) => Fixup { insn: insn + 3, operand: insn + 4, label: f.label, wide: true },
        _ => Fixup { insn, operand: insn + 1, ..*f },
    }
}

/// The offset of the 4-byte aligned operands of a switch at `at`.
fn aligned(at: usize) -> usize {
    (at + 4) & !3
}

/// The conditional jump taken when the one of `opcode` is not.
fn inverse(opcode: u8) -> u8 {
    if opcode < GOTO { ((opcode + 1) ^ 1) - 1 } else { opcode ^ 1 }
}
