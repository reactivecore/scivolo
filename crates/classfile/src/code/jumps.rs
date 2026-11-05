//! Labels and the jumps to them, with 16-bit offsets fixed up when a label is placed.

use super::Code;

/// A position in a method's code, placed once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Label(usize);

impl Code {
    pub fn new_label(&mut self) -> Label {
        self.labels.push(None);
        Label(self.labels.len() - 1)
    }

    /// Places `l` at the current offset and fixes up the jumps to it.
    pub fn place(&mut self, l: Label) {
        let here = self.bytes.len();
        self.labels[l.0] = Some(here);
        self.live = true;
        let (done, rest) = std::mem::take(&mut self.fixups).into_iter().partition(|f| f.1 == l);
        self.fixups = rest;
        for (at, _) in done {
            self.patch(at, here);
        }
    }

    /// Jump `opcode` (`ifeq` to `if_acmpne`, `goto`, `ifnull`, `ifnonnull`) to `target`.
    pub fn jump(&mut self, opcode: u8, target: Label) {
        let at = self.bytes.len();
        let delta = match opcode {
            0x99..=0x9e | 0xc6 | 0xc7 => -1,
            0x9f..=0xa6 => -2,
            _ => 0,
        };
        self.op(opcode, &[0, 0], delta);
        match self.labels[target.0] {
            Some(to) => self.patch(at, to),
            None => self.fixups.push((at, target)),
        }
    }

    /// Why the code cannot be written, if it cannot.
    pub fn problem(&self) -> Option<String> {
        let unplaced = (!self.fixups.is_empty()).then(|| "a jump to a label never placed".to_owned());
        self.error.clone().or(unplaced)
    }

    /// Writes the offset from the jump at `at` to `to` into its operand.
    fn patch(&mut self, at: usize, to: usize) {
        let Ok(offset) = i16::try_from(to as i64 - at as i64) else {
            self.error = Some("a jump over 32 KiB".into());
            return;
        };
        self.bytes[at + 1..at + 3].copy_from_slice(&offset.to_be_bytes());
    }
}
