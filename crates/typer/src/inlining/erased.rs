//! Erased definitions in inline code: `scala.compiletime.erasedValue` and the definitions
//! flagged `erased` (dotty's `isErased`).

use super::Inlining;
use tasty::tags::ERASED;
use tasty::{SymbolId, Universe};

/// Whether `sym` is `erasedValue` of `scala.compiletime` or `scala.caps`.
pub fn is_erased_value(u: &Universe, sym: SymbolId) -> bool {
    matches!(u.symbol(sym).name.as_str(), "erasedValue" | "unsafeErasedValue")
        && (u.full_name_starts_with(sym, "scala.compiletime.") || u.full_name_starts_with(sym, "scala.caps."))
}

impl Inlining<'_> {
    pub fn is_erased(&self, sym: SymbolId) -> bool {
        self.u.symbol(sym).flags.has(ERASED) || is_erased_value(self.u, sym)
    }
}
