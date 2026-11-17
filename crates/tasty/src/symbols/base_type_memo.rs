//! The base types `base_type` found, remembered while their context stays the same.

use super::SymbolId;
use crate::types::TypeId;
use crate::{MatchTypeRules, NullMode};
use rustc_hash::FxHashMap;

/// The base types found in one context, by type and base class.
#[derive(Default)]
pub struct BaseTypeMemo {
    context: Option<MemoContext>,
    found: FxHashMap<(TypeId, SymbolId), Option<TypeId>>,
}

/// What base types depend on beside the type and the base class.
#[derive(Clone, Copy, PartialEq)]
pub struct MemoContext {
    /// The universe's generation.
    pub generation: u64,
    /// The symbol comparisons are made in, which opaque types it sees through.
    pub owner: Option<SymbolId>,
    pub null_mode: NullMode,
    pub opaques_elided: bool,
    /// How many opaque types defined outside TASTy have their aliases.
    pub opaque_aliases: usize,
    pub rules: MatchTypeRules,
}

/// The state a remembered computation leaves as it is: its context, the binders, skolems and
/// opaque lifts made and the match type reductions taken.
#[derive(PartialEq)]
pub struct MemoEffects {
    pub context: Option<MemoContext>,
    pub binders: u32,
    pub opaque_lifts: u64,
    pub reductions: (u32, bool),
}

impl BaseTypeMemo {
    /// Gets the base type remembered for `key` in `context`.
    pub fn get(&self, context: MemoContext, key: (TypeId, SymbolId)) -> Option<Option<TypeId>> {
        (self.context == Some(context)).then(|| self.found.get(&key).copied()).flatten()
    }

    /// Remembers `found` for `key` in `context`, dropping what was found in another context.
    pub fn insert(&mut self, context: MemoContext, key: (TypeId, SymbolId), found: Option<TypeId>) {
        if self.context != Some(context) {
            self.found.clear();
            self.context = Some(context);
        }
        self.found.insert(key, found);
    }
}
