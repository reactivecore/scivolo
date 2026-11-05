//! A method's bytecode, with the stack depth and locals it needs.

mod descriptors;
mod jumps;
mod ops;

use descriptors::{kind, slots};
pub use descriptors::{method_slots, split_method, value_slots};
pub use jumps::Label;

use crate::pool::{Pool, Ref};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Invoke {
    Virtual,
    Special,
    Static,
    Interface,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FieldOp {
    GetStatic,
    PutStatic,
    GetField,
    PutField,
}

pub struct Code {
    pub bytes: Vec<u8>,
    stack: i32,
    pub max_stack: u16,
    pub max_locals: u16,
    /// The offset of each label, once placed.
    labels: Vec<Option<usize>>,
    /// The jumps to labels not yet placed: the jump's offset and its label.
    fixups: Vec<(usize, Label)>,
    /// Why the code cannot be written, e.g. a jump over 32 KiB.
    error: Option<String>,
    live: bool,
}

impl Code {
    /// The code of a method whose parameters, `this` included, take `params` slots.
    pub fn new(params: u16) -> Code {
        Code {
            bytes: Vec::new(),
            stack: 0,
            max_stack: 0,
            max_locals: params,
            labels: Vec::new(),
            fixups: Vec::new(),
            error: None,
            live: true,
        }
    }

    /// The stack depth at the current offset, in slots.
    pub fn stack(&self) -> i32 {
        self.stack
    }

    /// Sets the stack depth, at a label reached only by jumps or after code that does not
    /// fall through.
    pub fn set_stack(&mut self, slots: i32) {
        self.stack = slots;
    }

    /// Whether the code after the last instruction is reached by falling through it.
    pub fn live(&self) -> bool {
        self.live
    }

    /// Appends `opcode` and its operand bytes; the stack changes by `delta` slots.
    fn op(&mut self, opcode: u8, operands: &[u8], delta: i32) {
        self.live = !matches!(opcode, 0xa7 | 0xac..=0xb1 | 0xbf);
        self.bytes.push(opcode);
        self.bytes.extend_from_slice(operands);
        self.stack += delta;
        debug_assert!(self.stack >= 0, "stack underflow at opcode {opcode:#x}");
        self.max_stack = self.max_stack.max(u16::try_from(self.stack).unwrap_or(u16::MAX));
    }

    /// The load of local `slot` holding a value of descriptor `desc`.
    pub fn load(&mut self, desc: &str, slot: u16) {
        let kind = kind(desc);
        self.var(0x1a + 4 * kind, 0x15 + kind, slot, slots(desc.as_bytes()[0]));
    }

    /// The store into local `slot` of a value of descriptor `desc`.
    pub fn store(&mut self, desc: &str, slot: u16) {
        let kind = kind(desc);
        self.var(0x3b + 4 * kind, 0x36 + kind, slot, -slots(desc.as_bytes()[0]));
    }

    /// A load or store of `slot`: `short + slot` for slots 0 to 3, else `long slot`; the stack
    /// changes by `delta`, of one or two slots.
    fn var(&mut self, short: u8, long: u8, slot: u16, delta: i32) {
        let size = delta.unsigned_abs() as u16;
        self.max_locals = self.max_locals.max(slot.saturating_add(size));
        match slot {
            0..=3 => self.op(short + slot as u8, &[], delta),
            4..=255 => self.op(long, &[slot as u8], delta),
            _ => {
                self.bytes.push(0xc4);
                self.op(long, &slot.to_be_bytes(), delta);
            }
        }
    }

    /// `ldc` of one-slot constant pool entry `index`.
    pub fn ldc(&mut self, index: u16) {
        match u8::try_from(index) {
            Ok(i) => self.op(0x12, &[i], 1),
            Err(_) => self.op(0x13, &index.to_be_bytes(), 1),
        }
    }

    pub fn field(&mut self, op: FieldOp, pool: &mut Pool, owner: &str, name: &str, descriptor: &str) {
        let size = slots(descriptor.as_bytes()[0]);
        let (opcode, delta) = match op {
            FieldOp::GetStatic => (0xb2, size),
            FieldOp::PutStatic => (0xb3, -size),
            FieldOp::GetField => (0xb4, size - 1),
            FieldOp::PutField => (0xb5, -size - 1),
        };
        let index = pool.member(Ref::Field, owner, name, descriptor);
        self.op(opcode, &index.to_be_bytes(), delta);
    }

    pub fn invoke(&mut self, kind: Invoke, pool: &mut Pool, owner: &str, name: &str, descriptor: &str) {
        let (params, result) = method_slots(descriptor);
        let receiver = i32::from(kind != Invoke::Static);
        let delta = result - params - receiver;
        let r = if kind == Invoke::Interface { Ref::InterfaceMethod } else { Ref::Method };
        let index = pool.member(r, owner, name, descriptor).to_be_bytes();
        match kind {
            Invoke::Virtual => self.op(0xb6, &index, delta),
            Invoke::Special => self.op(0xb7, &index, delta),
            Invoke::Static => self.op(0xb8, &index, delta),
            Invoke::Interface => {
                let count = u8::try_from(params + 1).unwrap_or(u8::MAX);
                self.op(0xb9, &[index[0], index[1], count, 0], delta)
            }
        }
    }

    /// `new` of class `class`, an internal name.
    pub fn new_object(&mut self, pool: &mut Pool, class: &str) {
        let index = pool.class(class);
        self.op(0xbb, &index.to_be_bytes(), 1);
    }

    pub fn checkcast(&mut self, pool: &mut Pool, class: &str) {
        let index = pool.class(class);
        self.op(0xc0, &index.to_be_bytes(), 0);
    }

    pub fn dup(&mut self) {
        self.op(0x59, &[], 1);
    }

    pub fn pop(&mut self) {
        self.op(0x57, &[], -1);
    }

    pub fn aconst_null(&mut self) {
        self.op(0x01, &[], 1);
    }

    /// The return of a value of descriptor `result`, `V` for none.
    pub fn ret(&mut self, result: &str) {
        let (opcode, delta) = match result.as_bytes()[0] {
            b'V' => (0xb1, 0),
            b'L' | b'[' => (0xb0, -1),
            b'J' => (0xad, -2),
            b'F' => (0xae, -1),
            b'D' => (0xaf, -2),
            _ => (0xac, -1),
        };
        self.op(opcode, &[], delta);
    }
}
