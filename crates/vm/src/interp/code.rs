//! A method body lowered for execution: operands resolved to slots or constants, blocks to
//! instruction indices, numeric types to `Num`, and one cache cell per call site that
//! resolves something at run time.

use nir::{Bin, Comp, Conv, Defn, GlobalId, StrId, TypeId, ValId};
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug)]
pub enum Arg {
    Slot(u32),
    Imm(u64),
    /// A value the VM materializes on first use (strings, globals, class objects, constants).
    Lazy(u32),
}

/// Numeric kinds values are computed in; references and pointers are `I64`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Num {
    Bool,
    I8,
    I16,
    U16,
    I32,
    I64,
    F32,
    F64,
}

/// Where control continues: an instruction index plus the block parameters to assign.
#[derive(Debug)]
pub struct Target {
    pub pc: u32,
    pub moves: Box<[(u32, Arg)]>,
}

/// The block an exception thrown by an instruction continues in, and the slot it lands in.
pub type Handler = Option<Box<(u32, Target)>>;

#[derive(Debug)]
pub enum Instr {
    Bin {
        bin: Bin,
        num: Num,
        dst: u32,
        l: Arg,
        r: Arg,
        unwind: Handler,
    },
    Comp {
        comp: Comp,
        num: Num,
        dst: u32,
        l: Arg,
        r: Arg,
    },
    Conv {
        conv: Conv,
        from: Num,
        to: Num,
        dst: u32,
        v: Arg,
    },
    Copy {
        dst: u32,
        v: Arg,
    },
    Call {
        dst: u32,
        target: Arg,
        args: Box<[Arg]>,
        unwind: Handler,
    },
    /// A direct call of a method or extern, resolved to its function at `site`.
    CallGlobal {
        dst: u32,
        name: GlobalId,
        site: u32,
        args: Box<[Arg]>,
        unwind: Handler,
    },
    Method {
        dst: u32,
        obj: Arg,
        sig: StrId,
        site: u32,
        unwind: Handler,
    },
    Module {
        dst: u32,
        name: GlobalId,
        site: u32,
        unwind: Handler,
    },
    FieldLoad {
        dst: u32,
        ty: TypeId,
        obj: Arg,
        name: GlobalId,
        site: u32,
        unwind: Handler,
    },
    FieldStore {
        ty: TypeId,
        obj: Arg,
        name: GlobalId,
        v: Arg,
        site: u32,
        unwind: Handler,
    },
    ArrayLoad {
        dst: u32,
        ty: TypeId,
        arr: Arg,
        idx: Arg,
        unwind: Handler,
    },
    ArrayStore {
        ty: TypeId,
        arr: Arg,
        idx: Arg,
        v: Arg,
        unwind: Handler,
    },
    ArrayLength {
        dst: u32,
        arr: Arg,
        unwind: Handler,
    },
    Load {
        dst: u32,
        ty: TypeId,
        ptr: Arg,
    },
    Store {
        ty: TypeId,
        ptr: Arg,
        v: Arg,
    },
    Classalloc {
        dst: u32,
        name: GlobalId,
        site: u32,
        unwind: Handler,
    },
    Is {
        dst: u32,
        ty: TypeId,
        obj: Arg,
        site: u32,
    },
    /// Any other operation, executed from the NIR instruction at `inst`.
    Op {
        dst: u32,
        inst: u32,
        unwind: Handler,
    },
    Jump(Target),
    If {
        cond: Arg,
        then: Target,
        other: Target,
    },
    Switch {
        v: Arg,
        cases: Box<[(u64, Target)]>,
        default: Target,
    },
    /// `linktime if`, with the condition in the NIR instruction at `inst`.
    LinktimeIf {
        inst: u32,
        then: Target,
        other: Target,
    },
    Ret(Arg),
    Throw {
        v: Arg,
        unwind: Handler,
    },
    Unreachable,
}

pub struct Code {
    pub defn: Rc<Defn>,
    pub params: Box<[u32]>,
    pub slots: usize,
    pub instrs: Box<[Instr]>,
    pub lazy: Box<[(ValId, Cell<u64>)]>,
    pub sites: Box<[Cell<u64>]>,
}
