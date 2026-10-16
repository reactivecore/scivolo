//! References that inlining makes outside the subset: calls the typer or the `inlining` phase
//! would expand or reject, and references from inline bodies that need inline accessors.

use super::Typer;
use crate::ctx::Ctx;
use tasty::SymbolId;
use tasty::tags::{INLINE, MACRO, PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified};

/// The inline operations of `scala.compiletime` that inlining expands.
const EXPANDED: [&str; 9] = [
    "error",
    "codeOf",
    "requireConst",
    "constValue",
    "constValueOpt",
    "constValueTuple",
    "summonInline",
    "summonAll",
    "summonFrom",
];

impl Typer<'_> {
    /// Checks a reference to `sym` from `ctx`: not to a macro or an inline `scala.compiletime`
    /// operation, nor to `erasedValue` outside an inline method, nor from an inline body to a
    /// member that needs an inline accessor.
    pub fn check_inline_reference(&mut self, sym: SymbolId, ctx: Ctx) -> Result<(), &'static str> {
        let flags = self.u.symbol(sym).flags;
        if flags.has(INLINE) && flags.has(MACRO) {
            return Err("macro call");
        }
        if flags.has(INLINE) && self.u.within_opaques(sym) {
            return Err("inline method of an object with opaque types");
        }
        let full = self.u.full_name(sym);
        if flags.has(INLINE)
            && full.starts_with("scala.compiletime.")
            && !full
                .strip_prefix("scala.compiletime.package$package$.")
                .is_some_and(|n| EXPANDED.contains(&n))
        {
            return Err("scala.compiletime operation");
        }
        if crate::inlining::is_erased_value(self.u, sym) && self.enclosing_inline(ctx.owner).is_none() {
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
