//! Which locals the lowering may keep out of memory, judged from all their uses.

use nir::{Body, Inst, Next, Op, Pool, StrId, Val, ValId};
use rustc_hash::{FxHashMap, FxHashSet};

pub fn local(pool: &Pool, v: ValId) -> Option<u64> {
    match pool.val(v) {
        Val::Local(l, _) => Some(l.0),
        _ => None,
    }
}

/// Calls `f` with every local `v` reads, including those inside aggregate values.
pub fn locals(pool: &Pool, v: ValId, f: &mut impl FnMut(u64)) {
    match pool.val(v) {
        Val::Local(l, _) => f(l.0),
        Val::StructValue(vs) | Val::ArrayValue(_, vs) => vs.iter().for_each(|v| locals(pool, *v, f)),
        Val::Const(inner) => locals(pool, *inner, f),
        _ => {}
    }
}

/// How often each local is read.
pub fn use_counts(body: &Body, pool: &Pool) -> FxHashMap<u64, usize> {
    let mut uses: FxHashMap<u64, usize> = FxHashMap::default();
    for inst in &body.insts {
        inst.for_each_operand(&mut |v| locals(pool, v, &mut |l| *uses.entry(l).or_default() += 1));
    }
    uses
}

/// Removes from `candidates` the locals read other than as the first `skip(inst)` operands of
/// instructions, and returns the candidates read as one of those.
fn read_only_first(
    body: &Body,
    pool: &Pool,
    candidates: &mut FxHashSet<u64>,
    skip: impl Fn(&Inst) -> usize,
) -> FxHashSet<u64> {
    let mut first = FxHashSet::default();
    for inst in &body.insts {
        let (skip, mut at) = (skip(inst), 0);
        inst.for_each_operand(&mut |v| {
            match (at < skip, local(pool, v)) {
                (true, Some(l)) => {
                    first.insert(l);
                }
                _ => locals(pool, v, &mut |l| {
                    candidates.remove(&l);
                }),
            }
            at += 1;
        });
    }
    first
}

/// The locals holding a `method` result that is only ever called with the same receiver,
/// by that receiver and the method's signature, and is called at least once.
pub fn fusable_methods(body: &Body, pool: &Pool) -> FxHashMap<u64, (ValId, StrId)> {
    let methods: FxHashMap<u64, (ValId, StrId)> = body
        .insts
        .iter()
        .filter_map(|inst| match inst {
            Inst::Let { id, op: Op::Method { obj, sig }, .. } => Some((id.0, (*obj, *sig))),
            _ => None,
        })
        .collect();
    let mut kept: FxHashSet<u64> = methods.keys().copied().collect();
    let called = read_only_first(body, pool, &mut kept, |inst| match inst {
        Inst::Let { op: Op::Call { ptr, args, .. }, .. } => match local(pool, *ptr) {
            Some(m) if methods.get(&m).is_some_and(|(obj, _)| args.first() == Some(obj)) => 1,
            _ => 0,
        },
        _ => 0,
    });
    methods.into_iter().filter(|(l, _)| kept.contains(l) && called.contains(l)).collect()
}

/// The `var` cells used only as the cell of `varload` and `varstore`, which can live in a slot.
pub fn slot_vars(body: &Body, pool: &Pool) -> FxHashSet<u64> {
    let mut vars: FxHashSet<u64> = body
        .insts
        .iter()
        .filter_map(|inst| match inst {
            Inst::Let { id, op: Op::Var(_), .. } => Some(id.0),
            _ => None,
        })
        .collect();
    read_only_first(body, pool, &mut vars, |inst| match inst {
        Inst::Let { op: Op::Varload(_) | Op::Varstore { .. }, .. } => 1,
        _ => 0,
    });
    vars
}

/// Locals that are another value under a new name, with that value: the results of `copy`, and
/// the parameters of blocks entered by one jump only, which get that jump's arguments.
pub fn aliases(body: &Body) -> FxHashMap<u64, ValId> {
    let entries = entries(body);
    let params: FxHashMap<u64, &[nir::Param]> = body
        .insts
        .iter()
        .skip(1)
        .filter_map(|i| match i {
            Inst::Label(l, ps) => Some((l.0, ps.as_slice())),
            _ => None,
        })
        .collect();
    let mut aliases = FxHashMap::default();
    for inst in &body.insts {
        match inst {
            Inst::Let { id, op: Op::Copy(v), .. } => {
                aliases.insert(id.0, *v);
            }
            Inst::Jump(Next::Label(l, args)) if entries.get(&l.0) == Some(&1) => {
                if let Some(ps) = params.get(&l.0) {
                    aliases.extend(ps.iter().zip(args).map(|(p, a)| (p.id.0, *a)));
                }
            }
            _ => {}
        }
    }
    aliases
}

/// How many branches, jumps and handlers enter each block.
pub fn entries(body: &Body) -> FxHashMap<u64, usize> {
    let mut entries: FxHashMap<u64, usize> = FxHashMap::default();
    for inst in &body.insts {
        inst.for_each_successor(&mut |l| *entries.entry(l.0).or_default() += 1);
    }
    entries
}

/// The locals some alias stands for, directly or inside an aggregate.
pub fn aliased(pool: &Pool, aliases: &FxHashMap<u64, ValId>) -> FxHashSet<u64> {
    let mut out = FxHashSet::default();
    aliases.values().for_each(|v| {
        locals(pool, *v, &mut |l| {
            out.insert(l);
        })
    });
    out
}

/// Comparisons read only by the `if` right after them, which branches on them.
pub fn branch_comps(
    body: &Body,
    pool: &Pool,
    uses: &FxHashMap<u64, usize>,
    aliased: &FxHashSet<u64>,
) -> FxHashSet<u64> {
    body.insts
        .windows(2)
        .filter_map(|pair| match pair {
            [Inst::Let { id, op: Op::Comp { .. }, .. }, Inst::If(cond, ..)]
                if local(pool, *cond) == Some(id.0)
                    && uses.get(&id.0) == Some(&1)
                    && !aliased.contains(&id.0) =>
            {
                Some(id.0)
            }
            _ => None,
        })
        .collect()
}
