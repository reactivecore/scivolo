//! References that inlining makes outside the subset: calls the typer or the `inlining` phase
//! would expand or reject, and references from inline bodies that need inline accessors.

use super::Typer;
use crate::ctx::Ctx;
use tasty::SymbolId;
use tasty::tags::{INLINE, MACRO, PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified};

impl Typer<'_> {
    /// Checks a reference to `sym` from `ctx`: not to a macro or an inline `scala.compiletime`
    /// operation, nor from an inline body to a member that needs an inline accessor.
    pub fn check_inline_reference(&mut self, sym: SymbolId, ctx: Ctx) -> Result<(), &'static str> {
        let flags = self.u.symbol(sym).flags;
        if flags.has(INLINE) && flags.has(MACRO) {
            return Err("macro call");
        }
        if flags.has(INLINE) && self.u.full_name(sym).starts_with("scala.compiletime.") {
            return Err("scala.compiletime operation");
        }
        let full = self.u.full_name(sym);
        let erased_value = matches!(self.u.symbol(sym).name.as_str(), "erasedValue" | "unsafeErasedValue")
            && (full.starts_with("scala.compiletime.") || full.starts_with("scala.caps."));
        if erased_value {
            return Err("reference to an erased definition");
        }
        if self.needs_inline_accessor(sym, ctx) {
            return Err("non-public member in an inline method");
        }
        Ok(())
    }

    /// Whether a reference to `sym` from `ctx` is in the body of an inline method, outside which
    /// `sym` is not public, and `sym` is not defined in that method (dotty's `needsAccessor`).
    pub fn needs_inline_accessor(&self, sym: SymbolId, ctx: Ctx) -> bool {
        let flags = self.u.symbol(sym).flags;
        let restricted =
            [PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified].iter().any(|&t| flags.has(t))
                || self.syms.private_within.contains_key(&sym);
        if !restricted || self.u.is_type_symbol(sym) {
            return false;
        }
        let Some(inline) = self.enclosing_inline(ctx.owner) else { return false };
        let mut d = self.u.symbol(sym).owner;
        while let Some(x) = d {
            if x == inline {
                return false;
            }
            d = self.u.symbol(x).owner;
        }
        true
    }

    /// The inline method `owner` is in, if any.
    pub fn enclosing_inline(&self, owner: SymbolId) -> Option<SymbolId> {
        let mut o = Some(owner);
        while let Some(s) = o {
            let sym = self.u.symbol(s);
            if sym.kind == tasty::Kind::Def && sym.flags.has(INLINE) {
                return Some(s);
            }
            o = sym.owner;
        }
        None
    }
}
