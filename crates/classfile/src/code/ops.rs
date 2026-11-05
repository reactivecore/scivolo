//! Constants, operand-free instructions (arithmetic, conversions, comparisons, array elements)
//! and arrays.

use super::{Code, slots};
use crate::pool::Pool;

impl Code {
    /// Operand-free instruction `opcode` of the arithmetic, conversion, comparison, array element,
    /// `dup`/`pop` and `iconst` families.
    pub fn insn(&mut self, opcode: u8) {
        let delta = match opcode {
            0x01..=0x08 | 0x0b..=0x0d => 1,
            0x09 | 0x0a | 0x0e | 0x0f => 2,
            0x2f | 0x31 => 0,
            0x2e..=0x35 => -1,
            0x50 | 0x52 => -4,
            0x4f..=0x56 => -3,
            0x57 => -1,
            0x58 => -2,
            0x59 => 1,
            0x5c => 2,
            0x60..=0x73 => -1 - i32::from(opcode % 2),
            0x74..=0x77 => 0,
            0x78..=0x7d => -1,
            0x7e..=0x83 => -1 - i32::from(opcode % 2),
            0x85 | 0x87 | 0x8c | 0x8d => 1,
            0x88 | 0x89 | 0x8e | 0x90 => -1,
            0x86 | 0x8a | 0x8b | 0x8f | 0x91..=0x93 => 0,
            0x94 | 0x97 | 0x98 => -3,
            0x95 | 0x96 => -1,
            0xbe => 0,
            0xbf => -1,
            _ => panic!("not an operand-free instruction: {opcode:#x}"),
        };
        self.op(opcode, &[], delta);
    }

    pub fn int(&mut self, pool: &mut Pool, v: i32) {
        match v {
            -1..=5 => self.insn((0x03 + v) as u8),
            -128..=127 => self.op(0x10, &[v as u8], 1),
            -32768..=32767 => self.op(0x11, &(v as i16).to_be_bytes(), 1),
            _ => {
                let index = pool.integer(v);
                self.ldc(index)
            }
        }
    }

    pub fn long(&mut self, pool: &mut Pool, v: i64) {
        match v {
            0 | 1 => self.insn(0x09 + v as u8),
            _ => self.op(0x14, &pool.long(v).to_be_bytes(), 2),
        }
    }

    pub fn float(&mut self, pool: &mut Pool, v: f32) {
        match v {
            0.0 | 1.0 | 2.0 if v.is_sign_positive() => self.insn(0x0b + v as u8),
            _ => {
                let index = pool.float(v);
                self.ldc(index)
            }
        }
    }

    pub fn double(&mut self, pool: &mut Pool, v: f64) {
        match v {
            0.0 | 1.0 if v.is_sign_positive() => self.insn(0x0e + v as u8),
            _ => self.op(0x14, &pool.double(v).to_be_bytes(), 2),
        }
    }

    /// `iinc` of int local `slot` by `by`, or `None` past one-byte operands.
    pub fn iinc(&mut self, slot: u16, by: i32) -> Option<()> {
        let (slot, by) = (u8::try_from(slot).ok()?, i8::try_from(by).ok()?);
        self.op(0x84, &[slot, by as u8], 0);
        Some(())
    }

    /// The new array of element descriptor `elem`, its length on the stack.
    pub fn new_array(&mut self, pool: &mut Pool, elem: &str) {
        let atype = match elem.as_bytes()[0] {
            b'Z' => 4,
            b'C' => 5,
            b'F' => 6,
            b'D' => 7,
            b'B' => 8,
            b'S' => 9,
            b'I' => 10,
            b'J' => 11,
            _ => {
                let class = elem.strip_prefix('L').and_then(|e| e.strip_suffix(';')).unwrap_or(elem);
                let index = pool.class(class);
                return self.op(0xbd, &index.to_be_bytes(), 0);
            }
        };
        self.op(0xbc, &[atype], 0);
    }

    /// The load (`store` false) or store of an element of an array of element descriptor `elem`.
    pub fn element(&mut self, elem: &str, store: bool) {
        let kind = match elem.as_bytes()[0] {
            b'I' => 0,
            b'J' => 1,
            b'F' => 2,
            b'D' => 3,
            b'L' | b'[' => 4,
            b'Z' | b'B' => 5,
            b'C' => 6,
            _ => 7,
        };
        self.insn(if store { 0x4f } else { 0x2e } + kind);
    }

    pub fn instance_of(&mut self, pool: &mut Pool, class: &str) {
        let index = pool.class(class);
        self.op(0xc1, &index.to_be_bytes(), 0);
    }

    /// Drops a value of descriptor `desc`.
    pub fn drop_value(&mut self, desc: &str) {
        match slots(desc.as_bytes()[0]) {
            0 => {}
            1 => self.insn(0x57),
            _ => self.insn(0x58),
        }
    }
}
