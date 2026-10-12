//! Erased definitions in inline code: `scala.compiletime.erasedValue` and the definitions
//! flagged `erased` (dotty's `isErased`).

use super::Inlining;
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::ERASED;
use tasty::{SymbolId, Universe};

/// Whether `sym` is `erasedValue` of `scala.compiletime` or `scala.caps`.
pub fn is_erased_value(u: &Universe, sym: SymbolId) -> bool {
    let full = u.full_name(sym);
    matches!(u.symbol(sym).name.as_str(), "erasedValue" | "unsafeErasedValue")
        && (full.starts_with("scala.compiletime.") || full.starts_with("scala.caps."))
}

impl Inlining<'_> {
    pub fn is_erased(&self, sym: SymbolId) -> bool {
        self.u.symbol(sym).flags.has(ERASED) || is_erased_value(self.u, sym)
    }

    /// A reference to `erasedValue` in `t`.
    pub fn erased_value_in(&self, t: TNodeId) -> Option<TNodeId> {
        let reference = matches!(self.tpd.tree(t), T::Ident { .. } | T::Select { .. });
        if reference && self.tpd.sym(t).is_some_and(|s| is_erased_value(self.u, s)) {
            return Some(t);
        }
        self.tpd.children(t).into_iter().find_map(|c| self.erased_value_in(c))
    }
}
