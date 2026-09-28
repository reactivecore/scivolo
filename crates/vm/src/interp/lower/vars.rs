//! Loads of `var`s that can read the var's slot instead of a copy of it, and values that can be
//! written to it directly.

use super::uses::{local, locals};
use nir::{Body, Inst, Op, Pool, ValId};
use rustc_hash::{FxHashMap, FxHashSet};

/// Loads of slot `vars` whose every use follows in the same block before the next store to the
/// var, with the var's cell. Loads that an alias stands for (`aliased`) are left out: the
/// alias's uses are not counted here.
pub fn direct_var_reads(
    body: &Body,
    pool: &Pool,
    uses: &FxHashMap<u64, usize>,
    vars: &FxHashSet<u64>,
    aliased: &FxHashSet<u64>,
) -> FxHashMap<u64, ValId> {
    let mut direct = FxHashMap::default();
    // Loads of the current block not yet invalidated: load → (cell local, cell, uses seen).
    let mut open: FxHashMap<u64, (u64, ValId, usize)> = FxHashMap::default();
    for inst in &body.insts {
        if let Inst::Label(..) = inst {
            open.clear();
            continue;
        }
        if !open.is_empty() {
            inst.for_each_operand(&mut |v| {
                locals(pool, v, &mut |l| {
                    if let Some(entry) = open.get_mut(&l) {
                        entry.2 += 1;
                    }
                })
            });
            open.retain(|l, (_, cell, seen)| {
                let done = uses.get(l) == Some(seen);
                if done {
                    direct.insert(*l, *cell);
                }
                !done
            });
        }
        match inst {
            Inst::Let { id, op: Op::Varload(cell), .. } if !aliased.contains(&id.0) => {
                if let Some(c) = local(pool, *cell).filter(|c| vars.contains(c)) {
                    open.insert(id.0, (c, *cell, 0));
                }
            }
            Inst::Let { op: Op::Varstore { slot, .. }, .. } => {
                let stored = local(pool, *slot);
                open.retain(|_, (c, _, _)| Some(*c) != stored);
            }
            _ => {}
        }
    }
    direct
}

/// Values stored into a slot var by the instruction right after theirs and used nowhere else,
/// with the var's local; the value's instruction writes the var's slot. Values whose instruction
/// lowers to nothing (copies, var operations, `known` constants) are left out.
pub fn direct_var_writes(
    body: &Body,
    pool: &Pool,
    uses: &FxHashMap<u64, usize>,
    vars: &FxHashSet<u64>,
    known: &FxHashMap<u64, u64>,
) -> FxHashMap<u64, u64> {
    body.insts
        .windows(2)
        .filter_map(|pair| match pair {
            [Inst::Let { id, op, .. }, Inst::Let { op: Op::Varstore { slot, value }, .. }]
                if !matches!(op, Op::Copy(_) | Op::Varload(_) | Op::Varstore { .. } | Op::Var(_))
                    && !known.contains_key(&id.0)
                    && local(pool, *value) == Some(id.0)
                    && uses.get(&id.0) == Some(&1) =>
            {
                local(pool, *slot).filter(|c| vars.contains(c)).map(|c| (id.0, c))
            }
            _ => None,
        })
        .collect()
}
