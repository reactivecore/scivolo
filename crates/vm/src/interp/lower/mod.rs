//! Lowers a NIR method body to `Code`.

mod blocks;
mod compact;
mod constants;
mod finish;
mod op;
mod uses;
mod vars;

use super::code::{Code, Instr, Num};
use nir::{Defn, DefnKind, GlobalId, Inst, Pool, StrId, Type, Val, ValId};
use rustc_hash::FxHashMap;
use std::cell::Cell;
use std::rc::Rc;

struct Lowerer<'p> {
    pool: &'p Pool,
    /// Each block's first instruction and parameters, by local and slot.
    blocks: FxHashMap<u64, (u32, Vec<(u64, u32)>)>,
    /// Slots by local, numbered densely.
    slots: FxHashMap<u64, u32>,
    aliases: FxHashMap<u64, ValId>,
    /// Locals whose value is known, by value (`constants::constants`).
    constants: FxHashMap<u64, u64>,
    /// Locals written straight into a slot var, with the var (`vars::direct_var_writes`).
    writes: FxHashMap<u64, u64>,
    /// Comparisons lowered into the `if` that branches on them.
    branch_comps: rustc_hash::FxHashSet<u64>,
    insts: &'p [Inst],
    lazy: Vec<(ValId, Cell<u64>)>,
    lazy_ids: FxHashMap<ValId, u32>,
    sites: u32,
    nowhere: u32,
    fusable: FxHashMap<u64, (ValId, StrId)>,
    slot_vars: rustc_hash::FxHashSet<u64>,
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

/// The numeric kind of operand `v`.
pub fn val_num(pool: &Pool, v: ValId) -> Num {
    match pool.val(v) {
        Val::Local(_, ty) | Val::Zero(ty) => num(pool.ty(*ty)),
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

/// Lowers `defn`; `constructed` gives the address of a module whose construction has finished.
pub fn lower(defn: Rc<Defn>, pool: &mut Pool, constructed: &dyn Fn(GlobalId) -> Option<u64>) -> Code {
    let DefnKind::Define { body, .. } = &defn.kind else { unreachable!("only defines are lowered") };
    let constants = constants::constants(body, pool, constructed);
    let (mut code, mut slots, aliases) = lower_body(&defn, body, pool, constants.clone());
    let mut instrs = std::mem::take(&mut code.instrs).into_vec();
    finish::renumber_ops(&mut instrs, pool, &mut slots, &aliases, &constants);
    code.instrs = instrs.into_boxed_slice();
    code.slots = slots.len().max(1);
    code
}

fn lower_body<'p>(
    defn: &Rc<Defn>,
    body: &'p nir::Body,
    pool: &'p Pool,
    constants: FxHashMap<u64, u64>,
) -> (Code, FxHashMap<u64, u32>, FxHashMap<u64, ValId>) {
    let slot_vars = uses::slot_vars(body, pool);
    let use_counts = uses::use_counts(body, pool);
    let mut aliases = uses::aliases(body);
    let aliased = uses::aliased(pool, &aliases);
    aliases.extend(vars::direct_var_reads(body, pool, &use_counts, &slot_vars, &aliased));
    let writes = vars::direct_var_writes(body, pool, &use_counts, &slot_vars, &constants);
    let mut l = Lowerer {
        writes,
        pool,
        insts: &body.insts,
        blocks: FxHashMap::default(),
        slots: FxHashMap::default(),
        branch_comps: uses::branch_comps(body, pool, &use_counts, &uses::aliased(pool, &aliases)),
        aliases,
        constants,
        lazy: Vec::new(),
        lazy_ids: FxHashMap::default(),
        sites: 0,
        nowhere: 0,
        fusable: uses::fusable_methods(body, pool),
        slot_vars,
    };
    let mut pc = 0;
    for inst in &body.insts {
        match inst {
            Inst::Label(label, params) => {
                let slots: Vec<(u64, u32)> = params.iter().map(|p| (p.id.0, l.slot(p.id.0))).collect();
                l.blocks.insert(label.0, (pc, slots));
            }
            _ => pc += 1,
        }
    }
    l.nowhere = pc;
    let params = match body.insts.first() {
        Some(Inst::Label(_, params)) => params.iter().map(|p| l.slot(p.id.0)).collect(),
        _ => Box::default(),
    };
    let lowered = body.insts.iter().enumerate().filter(|(_, inst)| !matches!(inst, Inst::Label(..)));
    let (mut instrs, mut origin): (Vec<Instr>, Vec<u32>) =
        lowered.map(|(i, inst)| (l.inst(i as u32, inst), i as u32)).unzip();
    instrs.push(Instr::Unreachable);
    origin.push(u32::MAX);
    let (instrs, origin) = compact::compact(instrs, origin);
    let shortcut = super::shortcut::detect(&instrs, &params);
    let code = Code {
        shortcut,
        slots: 0,
        params,
        instrs: instrs.into_boxed_slice(),
        origin: origin.into_boxed_slice(),
        sites: (0..l.sites).map(|_| Cell::new(0)).collect(),
        lazy: l.lazy.into_boxed_slice(),
        defn: defn.clone(),
        calls: Cell::new(0),
        tier: 0,
        inlined: None,
    };
    (code, l.slots, l.aliases)
}
