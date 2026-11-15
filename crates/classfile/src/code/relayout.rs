//! Instructions that change size: jumps whose offset does not fit in two bytes (ASM's
//! `EXPAND_ASM_INSNS`: `goto` becomes `goto_w`, a conditional jump its inverse over a `goto_w`),
//! and `ldc`s whose constant moved to another pool index.

use super::Code;
use super::jumps::Fixup;
use crate::frames::length;
use crate::opcodes::{GOTO, GOTO_W};
use std::collections::BTreeSet;

const LDC: u8 = 0x12;
const LDC_W: u8 = 0x13;

impl Code {
    /// Rewrites the pool indices of the protected ranges' classes, then of the instructions in
    /// code order, through `entry`; an `ldc` takes the form its new index needs (ASM's
    /// `visitTryCatchBlock`, `visitLdcInsn`).
    pub fn remap(&mut self, entry: &mut dyn FnMut(u16) -> u16) {
        for h in self.handlers.iter_mut().filter(|h| h.3 != 0) {
            h.3 = entry(h.3);
        }

        let (mut new, mut resized, mut at) = (Vec::new(), false, 0);
        while at < self.bytes.len() {
            let len = length(&self.bytes, at);
            if let Some(insn) = remapped(&self.bytes[at..at + len], entry) {
                resized |= insn.len() != len;
                new.push((at, insn));
            }
            at += len;
        }

        if !resized {
            for (at, insn) in new {
                self.bytes[at..at + insn.len()].copy_from_slice(&insn);
            }
            return;
        }
        self.far.clear();
        let mut new = new.into_iter().peekable();
        let moved = self.relayout(|at, _| new.next_if(|n| n.0 == at).map(|n| n.1));
        self.move_offsets(&moved, &BTreeSet::new());
    }

    /// Rewrites the jumps that do not reach and moves the offsets after them, until all reach.
    pub fn widen(&mut self) {
        while !self.far.is_empty() {
            let far: BTreeSet<usize> = std::mem::take(&mut self.far).into_iter().collect();
            let moved = self.relayout(|at, insn| {
                far.contains(&at).then(|| match insn[0] {
                    GOTO => vec![GOTO_W, 0, 0, 0, 0],
                    op => vec![inverse(op), 0, 8, GOTO_W, 0, 0, 0, 0],
                })
            });
            self.move_offsets(&moved, &far);
        }
    }

    /// Moves the labels, lines and jumps to their offsets after relayout `moved`, which widened
    /// the jumps at `far`, and writes the jumps' offsets anew.
    fn move_offsets(&mut self, moved: &[usize], far: &BTreeSet<usize>) {
        for l in self.labels.iter_mut().flatten() {
            *l = moved[*l];
        }
        for line in &mut self.lines {
            line.0 = moved[usize::from(line.0)] as u16;
        }
        let jumps = std::mem::take(&mut self.jumps);
        self.jumps = jumps.iter().map(|f| moved_jump(f, far, moved, &self.bytes)).collect();
        for f in self.jumps.clone() {
            let to = self.offset(f.label).expect("a placed label");
            self.patch(&f, to);
        }
    }

    /// The code with each instruction `replace` gives bytes for replaced by them and switches
    /// padded anew; the new offset of each old instruction and of the end.
    fn relayout(&mut self, mut replace: impl FnMut(usize, &[u8]) -> Option<Vec<u8>>) -> Vec<usize> {
        let old = std::mem::take(&mut self.bytes);
        let mut moved = vec![0; old.len() + 1];
        let mut at = 0;
        while at < old.len() {
            let (op, len, here) = (old[at], length(&old, at), self.bytes.len());
            moved[at] = here;
            match replace(at, &old[at..at + len]) {
                Some(insn) => self.bytes.extend(insn),
                None if matches!(op, 0xaa | 0xab) => {
                    self.bytes.push(op);
                    self.bytes.resize(aligned(here), 0);
                    self.bytes.extend(&old[aligned(at)..at + len]);
                }
                None => self.bytes.extend(&old[at..at + len]),
            }
            at += len;
        }
        moved[old.len()] = self.bytes.len();
        moved
    }
}

/// Instruction `insn` with its pool index mapped through `entry`, none without one.
fn remapped(insn: &[u8], entry: &mut dyn FnMut(u16) -> u16) -> Option<Vec<u8>> {
    let index = || u16::from_be_bytes([insn[1], insn[2]]);
    Some(match insn[0] {
        LDC | LDC_W => {
            let i = entry(if insn[0] == LDC { u16::from(insn[1]) } else { index() });
            match u8::try_from(i) {
                Ok(i) => vec![LDC, i],
                Err(_) => [&[LDC_W][..], &i.to_be_bytes()].concat(),
            }
        }
        0x14 | 0xb2..=0xbb | 0xbd | 0xc0 | 0xc1 => {
            let mut out = insn.to_vec();
            out[1..3].copy_from_slice(&entry(index()).to_be_bytes());
            out
        }
        _ => return None,
    })
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

#[cfg(test)]
mod tests {
    use super::inverse;

    #[test]
    fn conditional_jumps_invert_in_pairs() {
        let pairs = [(0x99, 0x9a), (0x9b, 0x9c), (0x9d, 0x9e), (0x9f, 0xa0), (0xa1, 0xa2), (0xa3, 0xa4)];
        for (a, b) in pairs.into_iter().chain([(0xa5, 0xa6), (0xc6, 0xc7)]) {
            assert_eq!((inverse(a), inverse(b)), (b, a), "{a:#x}");
        }
    }
}
