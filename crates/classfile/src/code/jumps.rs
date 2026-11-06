//! Labels, the jumps and switches to them with offsets fixed up when a label is placed, and the
//! protected ranges of exception handlers.

use super::Code;

/// A position in a method's code, placed once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Label(usize);

/// A jump offset to write: the jump's offset, the operand's, its label, and whether it takes four
/// bytes.
pub struct Fixup {
    insn: usize,
    operand: usize,
    label: Label,
    wide: bool,
}

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
        let (done, rest) = std::mem::take(&mut self.fixups).into_iter().partition(|f| f.label == l);
        self.fixups = rest;
        for f in done {
            self.patch(&f, here);
        }
    }

    /// A new label placed at the current offset.
    pub fn here(&mut self) -> Label {
        let l = self.new_label();
        self.place(l);
        l
    }

    /// Whether `l` is placed at the current offset.
    pub fn is_here(&self, l: Label) -> bool {
        self.labels[l.0] == Some(self.bytes.len())
    }

    /// The offset of `l`, if placed.
    pub fn offset(&self, l: Label) -> Option<usize> {
        self.labels[l.0]
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
        self.offset_to(at, target, false);
    }

    /// `tableswitch` on the int on the stack: keys from `low` jump to `targets`, others to `default`.
    pub fn table_switch(&mut self, low: i32, default: Label, targets: &[Label]) {
        let at = self.switch_start(0xaa);
        self.offset_to(at, default, true);
        let high = low.wrapping_add(targets.len() as i32 - 1);
        self.bytes.extend(low.to_be_bytes().into_iter().chain(high.to_be_bytes()));
        for &t in targets {
            self.offset_to(at, t, true);
        }
    }

    /// `lookupswitch` on the int on the stack over `cases` sorted by key, else to `default`.
    pub fn lookup_switch(&mut self, default: Label, cases: &[(i32, Label)]) {
        let at = self.switch_start(0xab);
        self.offset_to(at, default, true);
        self.bytes.extend((cases.len() as u32).to_be_bytes());
        for &(key, t) in cases {
            self.bytes.extend(key.to_be_bytes());
            self.offset_to(at, t, true);
        }
    }

    /// Code from `start` to `end` handled at `handler` for exceptions of the class of pool index
    /// `catch`, 0 for any.
    pub fn protect(&mut self, start: Label, end: Label, handler: Label, catch: u16) {
        self.handlers.push((start, end, handler, catch));
    }

    /// The protected ranges as offsets: start, end, handler and class index.
    pub fn handler_offsets(&self) -> Option<Vec<[usize; 4]>> {
        let at = |l: Label| self.labels[l.0];
        let ranges = self.handlers.iter().map(|&(s, e, h, c)| Some([at(s)?, at(e)?, at(h)?, c.into()]));
        ranges.collect()
    }

    /// Why the code cannot be written, if it cannot.
    pub fn problem(&self) -> Option<String> {
        let unplaced = (!self.fixups.is_empty()).then(|| "a jump to a label never placed".to_owned());
        self.error.clone().or(unplaced)
    }

    /// Switch `opcode` and its padding; returns the switch's offset.
    fn switch_start(&mut self, opcode: u8) -> usize {
        let at = self.bytes.len();
        self.op(opcode, &[], -1);
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        at
    }

    /// The operand at the current offset of the jump at `insn` to `target`.
    fn offset_to(&mut self, insn: usize, target: Label, wide: bool) {
        let operand = if wide { self.bytes.len() } else { insn + 1 };
        if wide {
            self.bytes.extend([0; 4]);
        }
        let f = Fixup { insn, operand, label: target, wide };
        match self.labels[target.0] {
            Some(to) => self.patch(&f, to),
            None => self.fixups.push(f),
        }
    }

    /// Writes the offset from the jump of `f` to `to` into its operand.
    fn patch(&mut self, f: &Fixup, to: usize) {
        let offset = to as i64 - f.insn as i64;
        if f.wide {
            let offset = (offset as i32).to_be_bytes();
            self.bytes[f.operand..f.operand + 4].copy_from_slice(&offset);
            return;
        }
        let Ok(offset) = i16::try_from(offset) else {
            self.error = Some("a jump over 32 KiB".into());
            return;
        };
        self.bytes[f.operand..f.operand + 2].copy_from_slice(&offset.to_be_bytes());
    }
}
