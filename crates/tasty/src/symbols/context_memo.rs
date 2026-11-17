//! Results about types remembered while the universe and the context of comparisons stay the
//! same: base types (dotty's `baseTypeCache`) and member types as seen from a prefix.

use super::{SymbolId, Universe};
use crate::{MatchTypeRules, NullMode};
use rustc_hash::FxHashMap;
use std::hash::Hash;

/// The results found in one context, by their arguments.
pub struct ContextMemo<K, V> {
    context: Option<MemoContext>,
    found: FxHashMap<K, V>,
}

impl<K, V> Default for ContextMemo<K, V> {
    fn default() -> Self {
        ContextMemo { context: None, found: FxHashMap::default() }
    }
}

/// What the results depend on beside their arguments.
#[derive(Clone, Copy, PartialEq)]
struct MemoContext {
    /// The universe's generation.
    generation: u64,
    /// The symbol comparisons are made in, which opaque types it sees through.
    owner: Option<SymbolId>,
    null_mode: NullMode,
    opaques_elided: bool,
    /// How many opaque types defined outside TASTy have their aliases.
    opaque_aliases: usize,
    rules: MatchTypeRules,
}

/// The state a remembered computation leaves as it is: its context, the binders, skolems and
/// opaque lifts made and the match type reductions taken.
#[derive(PartialEq)]
struct MemoEffects {
    context: Option<MemoContext>,
    binders: u32,
    opaque_lifts: u64,
    reductions: (u32, bool),
}

/// Runs `compute` for `key`, or gets what it gave for `key` in the same context, kept in the
/// memo `memo` selects. A result is kept when computing it left the context as it was and made
/// no binder, skolem or opaque lift and took no match type reduction, and no refused reduction is
/// recorded (`reductions_exhausted`); none is kept or used while a GADT constraint, a merge of
/// base types, a resolution or a reduction is in progress.
pub fn remembered<K: Hash + Eq, V: Copy>(
    u: &mut Universe,
    memo: fn(&mut Universe) -> &mut ContextMemo<K, V>,
    key: K,
    compute: impl FnOnce(&mut Universe) -> V,
) -> V {
    let Some(context) = context(u) else { return compute(u) };
    let m = memo(u);
    if m.context == Some(context)
        && let Some(&found) = m.found.get(&key)
    {
        return found;
    }

    let before = effects(u);
    let found = compute(u);
    if effects(u) == before && !u.reductions_exhausted {
        let m = memo(u);
        if m.context != Some(context) {
            m.found.clear();
            m.context = Some(context);
        }
        m.found.insert(key, found);
    }
    found
}

fn context(u: &Universe) -> Option<MemoContext> {
    let free = u.gadt.is_empty() && u.merging_bases.is_empty() && u.resolving == 0 && u.reducing == 0;
    free.then_some(MemoContext {
        generation: u.generation,
        owner: u.comparison_owner,
        null_mode: u.null_mode,
        opaques_elided: u.opaques_elided,
        opaque_aliases: u.opaque_aliases.len(),
        rules: u.match_type_rules,
    })
}

fn effects(u: &Universe) -> MemoEffects {
    MemoEffects {
        context: context(u),
        binders: u.binders,
        opaque_lifts: u.opaque_lifts,
        reductions: (u.reductions_left, u.reductions_exhausted),
    }
}
