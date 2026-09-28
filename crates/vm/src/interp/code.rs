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

/// How a load or store accesses memory: a scalar of 1, 2, 4 or 8 bytes, or an aggregate of the
/// given size, whose value is its address.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mem {
    B1,
    B2,
    B4,
    B8,
    Aggregate(u32),
}

impl Mem {
    pub fn of(pool: &nir::Pool, ty: TypeId) -> Mem {
        let size = crate::layout::size_of(pool, ty);
        match (pool.ty(ty), size) {
            (nir::Type::StructValue(_) | nir::Type::ArrayValue(..), size) => Mem::Aggregate(size),
            (_, 1) => Mem::B1,
            (_, 2) => Mem::B2,
            (_, 4) => Mem::B4,
            _ => Mem::B8,
        }
    }

    pub fn size(self) -> usize {
        match self {
            Mem::B1 => 1,
            Mem::B2 => 2,
            Mem::B4 => 4,
            Mem::B8 => 8,
            Mem::Aggregate(size) => size as usize,
        }
    }
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
    /// A call through a function value of function type `ty`.
    Call {
        dst: u32,
        ty: TypeId,
        target: Arg,
        args: Box<[Arg]>,
        unwind: Handler,
    },
    /// A call of variadic C function `name`, with the numeric kind of each argument and of the
    /// result.
    CallVariadic {
        dst: u32,
        name: GlobalId,
        args: Box<[(Arg, Num)]>,
        ret: Num,
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
    /// A call of `BoxesRunTime.equals(Object, Object)`, whose last two arguments are compared
    /// directly when they are the same reference or boxes of the same `java.lang` class.
    BoxedEquals {
        dst: u32,
        name: GlobalId,
        site: u32,
        args: Box<[Arg]>,
        unwind: Handler,
    },
    /// A virtual call: `method args[0], sig` then the call, with the receiver's class and the
    /// implementation cached at `site` and `site + 1`.
    Invoke {
        dst: u32,
        sig: StrId,
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
        mem: Mem,
        obj: Arg,
        name: GlobalId,
        site: u32,
        unwind: Handler,
    },
    FieldStore {
        mem: Mem,
        obj: Arg,
        name: GlobalId,
        v: Arg,
        site: u32,
        unwind: Handler,
    },
    ArrayLoad {
        dst: u32,
        mem: Mem,
        arr: Arg,
        idx: Arg,
        unwind: Handler,
    },
    ArrayStore {
        mem: Mem,
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
        mem: Mem,
        ptr: Arg,
    },
    Store {
        mem: Mem,
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
    /// `base + idx * scale + offset`, with `idx` sign-extended from `idx_num`.
    Elem {
        dst: u32,
        base: Arg,
        idx: Arg,
        idx_num: Num,
        scale: u32,
        offset: i64,
    },
    As {
        dst: u32,
        ty: TypeId,
        obj: Arg,
        site: u32,
        unwind: Handler,
    },
    /// `box` or `unbox`, calling the helper cached at `site` and `site + 1`.
    Box {
        dst: u32,
        ty: TypeId,
        boxing: bool,
        v: Arg,
        site: u32,
        unwind: Handler,
    },
    /// A NIR operation without its own instruction; its locals are slot numbers.
    Op {
        dst: u32,
        op: Box<nir::Op>,
        unwind: Handler,
    },
    Jump(Target),
    If {
        cond: Arg,
        then: Target,
        other: Target,
    },
    /// A comparison whose only use is the branch on it.
    CompIf {
        comp: Comp,
        num: Num,
        l: Arg,
        r: Arg,
        then: Target,
        other: Target,
    },
    Switch {
        v: Arg,
        cases: Box<[(Arg, Target)]>,
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
    /// Nothing, left where a `method` was fused into the `Invoke` after it.
    Nop,
}

pub struct Code {
    pub defn: Rc<Defn>,
    pub params: Box<[u32]>,
    pub slots: usize,
    pub instrs: Box<[Instr]>,
    /// The index of the NIR instruction each of `instrs` was lowered from.
    pub origin: Box<[u32]>,
    pub lazy: Box<[(ValId, Cell<u64>)]>,
    pub shortcut: Option<super::shortcut::Shortcut>,
    pub sites: Box<[Cell<u64>]>,
    /// Calls so far, counted up to `speculate::TIER_UP`.
    pub calls: Cell<u32>,
    /// 0 for the first lowering, 1 after tier-up.
    pub tier: u8,
}

impl Code {
    /// What a frame header records of the method this code belongs to: its name and tier.
    pub fn frame_method(&self) -> u64 {
        self.defn.name.0 as u64 | (self.tier as u64) << 32
    }
}

impl Instr {
    /// Where an exception thrown by this instruction continues, if in this method.
    pub fn unwind(&self) -> &Handler {
        match self {
            Instr::Bin { unwind, .. }
            | Instr::Call { unwind, .. }
            | Instr::CallGlobal { unwind, .. }
            | Instr::BoxedEquals { unwind, .. }
            | Instr::Invoke { unwind, .. }
            | Instr::CallVariadic { unwind, .. }
            | Instr::Method { unwind, .. }
            | Instr::Module { unwind, .. }
            | Instr::FieldLoad { unwind, .. }
            | Instr::FieldStore { unwind, .. }
            | Instr::ArrayLoad { unwind, .. }
            | Instr::ArrayStore { unwind, .. }
            | Instr::ArrayLength { unwind, .. }
            | Instr::Classalloc { unwind, .. }
            | Instr::As { unwind, .. }
            | Instr::Box { unwind, .. }
            | Instr::Op { unwind, .. }
            | Instr::Throw { unwind, .. } => unwind,
            _ => &None,
        }
    }

    fn unwind_mut(&mut self) -> Option<&mut Handler> {
        match self {
            Instr::Bin { unwind, .. }
            | Instr::Call { unwind, .. }
            | Instr::CallGlobal { unwind, .. }
            | Instr::BoxedEquals { unwind, .. }
            | Instr::Invoke { unwind, .. }
            | Instr::CallVariadic { unwind, .. }
            | Instr::Method { unwind, .. }
            | Instr::Module { unwind, .. }
            | Instr::FieldLoad { unwind, .. }
            | Instr::FieldStore { unwind, .. }
            | Instr::ArrayLoad { unwind, .. }
            | Instr::ArrayStore { unwind, .. }
            | Instr::ArrayLength { unwind, .. }
            | Instr::Classalloc { unwind, .. }
            | Instr::As { unwind, .. }
            | Instr::Box { unwind, .. }
            | Instr::Op { unwind, .. }
            | Instr::Throw { unwind, .. } => Some(unwind),
            _ => None,
        }
    }

    /// Every place this instruction may continue at other than the next instruction.
    pub fn targets_mut(&mut self) -> Vec<&mut Target> {
        match self {
            Instr::Jump(t) => vec![t],
            Instr::If { then, other, .. }
            | Instr::CompIf { then, other, .. }
            | Instr::LinktimeIf { then, other, .. } => vec![then, other],
            Instr::Switch { cases, default, .. } => {
                cases.iter_mut().map(|(_, t)| t).chain(std::iter::once(default)).collect()
            }
            other => other.unwind_mut().and_then(|h| h.as_mut()).map(|h| &mut h.1).into_iter().collect(),
        }
    }
}
