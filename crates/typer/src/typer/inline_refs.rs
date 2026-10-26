// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/PrepareInlineable.scala
//! References that inlining makes outside the subset: calls the typer or the `inlining` phase
//! would expand or reject, and references from inline bodies that need inline accessors.

use super::Typer;
use crate::ctx::Ctx;
use tasty::SymbolId;
use tasty::tags::{INLINE, MACRO, PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified};

/// The inline operations of `scala.compiletime` that inlining expands.
const EXPANDED: [&str; 10] = [
    "asMatchable",
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

/// `scala.compiletime.testing.typeChecks` and `typeCheckErrors`, which inlining expands.
const TYPE_CHECKS: [&str; 2] = [
    "scala.compiletime.testing.package$package$.typeChecks",
    "scala.compiletime.testing.package$package$.typeCheckErrors",
];

impl Typer<'_> {
    /// Checks a reference to `sym` from `ctx`: not to a macro or an inline `scala.compiletime`
    /// operation, nor to `erasedValue` outside an inline method, nor from an inline body to a
    /// member that needs an inline accessor.
    pub fn check_inline_reference(&mut self, sym: SymbolId, ctx: Ctx) -> Result<(), &'static str> {
        let flags = self.u.symbol(sym).flags;
        if flags.has(INLINE) && flags.has(MACRO) && self.macros.is_none() {
            return Err("macro call");
        }
        if flags.has(INLINE) && self.u.is_deferred(sym) && self.enclosing_inline(ctx.owner).is_none() {
            return Err("call of a deferred inline method");
        }
        let full = self.u.full_name(sym);
        if flags.has(INLINE)
            && full.starts_with("scala.compiletime.")
            && !full
                .strip_prefix("scala.compiletime.package$package$.")
                .is_some_and(|n| EXPANDED.contains(&n))
            && !TYPE_CHECKS.contains(&full.as_str())
        {
            return Err("scala.compiletime operation");
        }
        if crate::inlining::is_erased_value(self.u, sym) && self.enclosing_inline(ctx.owner).is_none() {
            return Err("reference to an erased definition");
        }
        Ok(())
    }

    /// Whether a reference to `sym` from `ctx` needs an inline accessor.
    pub fn needs_inline_accessor(&mut self, sym: SymbolId, ctx: Ctx) -> bool {
        self.needs_inline_accessor_from(sym, ctx.owner)
    }

    /// Whether a reference to `sym` from `owner` is in the body of an inline member method,
    /// outside which `sym` is not public, and `sym` is not defined in that method, nor an inline
    /// method, nor a constant value, nor `@publicInBinary` (dotty's `needsAccessor`).
    pub fn needs_inline_accessor_from(&mut self, sym: SymbolId, owner: SymbolId) -> bool {
        let s = self.u.symbol(sym);
        let flags = s.flags;
        let restricted =
            [PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified].iter().any(|&t| flags.has(t))
                || self.syms.private_within.contains_key(&sym);
        if !restricted
            || self.u.is_type_symbol(sym)
            || s.kind == tasty::Kind::Def && flags.has(INLINE)
            || self.u.is_annotated(sym, "scala.annotation.publicInBinary")
        {
            return false;
        }
        let Some(inline) = self.enclosing_inline(owner) else { return false };
        if s.kind == tasty::Kind::Val && !flags.has(tasty::tags::MUTABLE) {
            self.complete(sym);
            let info = self.u.info(sym);
            if matches!(self.u.types().get(info), tasty::Type::Constant(..) | tasty::Type::Str(_)) {
                return false;
            }
        }
        if self.u.symbol(inline).owner.is_some_and(|o| self.u.symbol(o).kind.is_term()) {
            return false;
        }
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
