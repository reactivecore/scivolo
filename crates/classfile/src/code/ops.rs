//! Constants, operand-free instructions (arithmetic, conversions, comparisons, array elements)
//! and arrays.

use super::{Code, method_slots};
use crate::kind::Kind;
use crate::opcodes::{ANEWARRAY, IALOAD, IASTORE, IRETURN, NEWARRAY, POP, POP2, RETURN};
use crate::pool::{Constant, Handle, Pool};

impl Code {
    /// Operand-free instruction `opcode` of the arithmetic, conversion, comparison, array element,
    /// `dup`/`pop` and `iconst` families, `nop` and the monitor instructions.
    pub fn insn(&mut self, opcode: u8) {
        let delta = match opcode {
            0x00 => 0,
            0xc2 | 0xc3 => -1,
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

    /// `iinc` of int local `slot` by `by`, `wide` past one-byte operands; `None` past two-byte
    /// increments.
    pub fn iinc(&mut self, slot: u16, by: i32) -> Option<()> {
        let by = i16::try_from(by).ok()?;
        self.max_locals = self.max_locals.max(slot.saturating_add(1));
        match (u8::try_from(slot), i8::try_from(by)) {
            (Ok(slot), Ok(by)) => self.op(0x84, &[slot, by as u8], 0),
            _ => {
                let [s0, s1] = slot.to_be_bytes();
                let [b0, b1] = by.to_be_bytes();
                self.bytes.push(0xc4);
                self.op(0x84, &[s0, s1, b0, b1], 0);
            }
        }
        Some(())
    }

    /// The new array of element descriptor `elem`, its length on the stack.
    pub fn new_array(&mut self, pool: &mut Pool, elem: &str) {
        let Some(atype) = Kind::of(elem).array_type() else {
            let class = elem.strip_prefix('L').and_then(|e| e.strip_suffix(';')).unwrap_or(elem);
            let index = pool.class(class);
            return self.op(ANEWARRAY, &index.to_be_bytes(), 0);
        };
        self.op(NEWARRAY, &[atype], 0);
    }

    /// The load (`store` false) or store of an element of an array of element descriptor `elem`.
    pub fn element(&mut self, elem: &str, store: bool) {
        self.insn(if store { IASTORE } else { IALOAD } + Kind::of(elem).element_offset());
    }

    pub fn instance_of(&mut self, pool: &mut Pool, class: &str) {
        let index = pool.class(class);
        self.op(0xc1, &index.to_be_bytes(), 0);
    }

    /// `invokedynamic` of call site `name` of type `descriptor` that `bootstrap` links with
    /// arguments `args`.
    pub fn invoke_dynamic(
        &mut self,
        pool: &mut Pool,
        (name, descriptor): (&str, &str),
        bootstrap: &Handle,
        args: &[Constant],
    ) {
        let [i0, i1] = pool.invoke_dynamic(name, descriptor, bootstrap, args).to_be_bytes();
        let (params, result) = method_slots(descriptor);
        self.op(0xba, &[i0, i1, 0, 0], result - params);
    }

    /// The return of a value of descriptor `result`, `V` for none.
    pub fn ret(&mut self, result: &str) {
        let kind = Kind::of(result);
        let opcode = match kind {
            Kind::Void => RETURN,
            _ => IRETURN + kind.offset(),
        };
        self.op(opcode, &[], -kind.slots());
    }

    /// Drops a value of descriptor `desc`.
    pub fn drop_value(&mut self, desc: &str) {
        match Kind::of(desc).slots() {
            0 => {}
            1 => self.insn(POP),
            _ => self.insn(POP2),
        }
    }
}
