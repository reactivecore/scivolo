//! Lowers a NIR method body to `Code`.

mod op;

use super::code::{Arg, Code, Handler, Instr, Num, Target};
use nir::{Defn, DefnKind, Inst, Next, Op, Pool, StrId, Type, TypeId, Val, ValId};
use rustc_hash::FxHashMap;
use std::cell::Cell;
use std::rc::Rc;

struct Lowerer<'p> {
    pool: &'p Pool,
    blocks: FxHashMap<u64, (u32, Vec<u32>)>,
    lazy: Vec<(ValId, Cell<u64>)>,
    lazy_ids: FxHashMap<ValId, u32>,
    sites: u32,
    max_slot: u32,
    nowhere: u32,
    fusable: FxHashMap<u64, (ValId, StrId)>,
}

pub fn num(ty: &Type) -> Num {
    match ty {
        Type::Bool => Num::Bool,
        Type::Byte => Num::I8,
        Type::Short => Num::I16,
        Type::Char => Num::U16,
        Type::Int => Num::I32,
        Type::Float => Num::F32,
        Type::Double => Num::F64,
        _ => Num::I64,
    }
}

/// The value of a literal operand, as the zero-extended bits of its type.
pub fn immediate(v: &Val) -> Option<u64> {
    Some(match v {
        Val::True => 1,
        Val::False | Val::Null | Val::Zero(_) => 0,
        Val::Int(i) => *i as u32 as u64,
        Val::Long(l) => *l as u64,
        Val::Char(c) => *c as u64,
        Val::Byte(b) => *b as u8 as u64,
        Val::Short(s) => *s as u16 as u64,
        Val::Float(f) => *f as u64,
        Val::Double(d) | Val::Size(d) => *d,
        _ => return None,
    })
}

pub fn lower(defn: Rc<Defn>, pool: &Pool) -> Code {
    let DefnKind::Define { body, .. } = &defn.kind else { unreachable!("only defines are lowered") };
    let mut l = Lowerer {
        pool,
        blocks: FxHashMap::default(),
        lazy: Vec::new(),
        lazy_ids: FxHashMap::default(),
        sites: 0,
        max_slot: 0,
        nowhere: 0,
        fusable: FxHashMap::default(),
    };
    let mut pc = 0;
    for inst in &body.insts {
        match inst {
            Inst::Label(label, params) => {
                let slots: Vec<u32> = params.iter().map(|p| l.slot(p.id.0)).collect();
                l.blocks.insert(label.0, (pc, slots));
            }
            _ => pc += 1,
        }
    }
    l.nowhere = pc;
    l.fusable = fusable_methods(body, pool);
    let params = match body.insts.first() {
        Some(Inst::Label(_, params)) => params.iter().map(|p| p.id.0 as u32).collect(),
        _ => Box::default(),
    };
    let lowered = body.insts.iter().enumerate().filter(|(_, inst)| !matches!(inst, Inst::Label(..)));
    let (mut instrs, mut origin): (Vec<Instr>, Vec<u32>) =
        lowered.map(|(i, inst)| (l.inst(i as u32, inst), i as u32)).unzip();
    instrs.push(Instr::Unreachable);
    origin.push(u32::MAX);
    let shortcut = super::shortcut::detect(&instrs, &params);
    Code {
        shortcut,
        slots: l.max_slot as usize + 1,
        params,
        instrs: instrs.into_boxed_slice(),
        origin: origin.into_boxed_slice(),
        sites: (0..l.sites).map(|_| Cell::new(0)).collect(),
        lazy: l.lazy.into_boxed_slice(),
        defn: defn.clone(),
    }
}

/// The locals holding a `method` result whose only uses are as call targets, by the
/// receiver and signature of that `method`.
fn fusable_methods(body: &nir::Body, pool: &Pool) -> FxHashMap<u64, (ValId, StrId)> {
    let local = |v: ValId| match pool.val(v) {
        Val::Local(l, _) => Some(l.0),
        _ => None,
    };
    let mut methods = FxHashMap::default();
    let mut other_uses = rustc_hash::FxHashSet::default();
    for inst in &body.insts {
        if let Inst::Let { id, op: Op::Method { obj, sig }, .. } = inst {
            methods.insert(id.0, (*obj, *sig));
        }
        let ops = inst.operands();
        let skip = match inst {
            Inst::Let { op: Op::Call { .. }, .. } => 1,
            _ => 0,
        };
        other_uses.extend(ops.into_iter().skip(skip).filter_map(local));
    }
    methods.retain(|l, _| !other_uses.contains(l));
    methods
}

impl Lowerer<'_> {
    fn slot(&mut self, local: u64) -> u32 {
        self.max_slot = self.max_slot.max(local as u32);
        local as u32
    }

    fn site(&mut self) -> u32 {
        self.sites += 1;
        self.sites - 1
    }

    fn arg(&mut self, v: ValId) -> Arg {
        match self.pool.val(v) {
            Val::Local(l, _) => Arg::Slot(l.0 as u32),
            other => match immediate(other) {
                Some(bits) => Arg::Imm(bits),
                None => {
                    let next = self.lazy.len() as u32;
                    let id = *self.lazy_ids.entry(v).or_insert(next);
                    if id == next {
                        self.lazy.push((v, Cell::new(0)));
                    }
                    Arg::Lazy(id)
                }
            },
        }
    }

    fn args(&mut self, vs: &[ValId]) -> Box<[Arg]> {
        vs.iter().map(|&v| self.arg(v)).collect()
    }

    fn val_num(&self, v: ValId) -> Num {
        match self.pool.val(v) {
            Val::Local(_, ty) | Val::Zero(ty) => num(self.pool.ty(*ty)),
            Val::True | Val::False => Num::Bool,
            Val::Byte(_) => Num::I8,
            Val::Short(_) => Num::I16,
            Val::Char(_) => Num::U16,
            Val::Int(_) => Num::I32,
            Val::Float(_) => Num::F32,
            Val::Double(_) => Num::F64,
            _ => Num::I64,
        }
    }

    fn num_of(&self, ty: TypeId) -> Num {
        num(self.pool.ty(ty))
    }

    /// Where `next` continues; a missing label continues at the trailing `Unreachable`.
    fn target(&mut self, next: &Next) -> Target {
        let nowhere = Target { pc: self.nowhere, moves: Box::default() };
        let Next::Label(label, args) = next else { return nowhere };
        let Some((pc, params)) = self.blocks.get(&label.0).cloned() else { return nowhere };
        let moves = params.into_iter().zip(args).map(|(p, &a)| (p, self.arg(a))).collect();
        Target { pc, moves }
    }

    fn handler(&mut self, unwind: &Next) -> Handler {
        match unwind {
            Next::Unwind(param, to) => {
                let slot = self.slot(param.id.0);
                Some(Box::new((slot, self.target(to))))
            }
            _ => None,
        }
    }

    fn inst(&mut self, at: u32, inst: &Inst) -> Instr {
        match inst {
            Inst::Let { id, op, unwind, .. } => {
                let dst = self.slot(id.0);
                self.op(at, dst, op, unwind)
            }
            Inst::Ret(v) => Instr::Ret(self.arg(*v)),
            Inst::Jump(next) => Instr::Jump(self.target(next)),
            Inst::If(cond, then, other) => {
                Instr::If { cond: self.arg(*cond), then: self.target(then), other: self.target(other) }
            }
            Inst::Switch(v, default, cases) => {
                let cases = cases
                    .iter()
                    .filter_map(|c| match c {
                        Next::Case(value, to) => Some((self.arg(*value), self.target(to))),
                        _ => None,
                    })
                    .collect();
                Instr::Switch { v: self.arg(*v), cases, default: self.target(default) }
            }
            Inst::Throw(v, unwind) => Instr::Throw { v: self.arg(*v), unwind: self.handler(unwind) },
            Inst::LinktimeIf(_, then, other) => {
                Instr::LinktimeIf { inst: at, then: self.target(then), other: self.target(other) }
            }
            Inst::Unreachable(_) | Inst::Label(..) => Instr::Unreachable,
        }
    }
}
