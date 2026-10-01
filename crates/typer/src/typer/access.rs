//! Access to private and protected members (dotty's `isAccessibleFrom`, as far as the subset
//! goes: a restricted member is accessible from inside its class).

use super::Typer;
use crate::ctx::Ctx;
use tasty::SymbolId;
use tasty::tags::{PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified};

impl Typer<'_> {
    /// Checks a reference to `sym` from `ctx`: accessible, and not to an inline method, whose
    /// calls are expanded (a later step).
    pub fn check_reference(&mut self, sym: SymbolId, ctx: Ctx) -> Result<(), &'static str> {
        if !self.is_accessible(sym, ctx) {
            return Err("inaccessible member");
        }
        if self.u.symbol(sym).flags.has(tasty::tags::INLINE) {
            return Err("inline method call");
        }
        Ok(())
    }

    /// Whether `sym` may be referred to from `ctx`: it is unrestricted, or `ctx` is inside its
    /// owner, or, if it is protected, inside a subclass of its owner.
    pub fn is_accessible(&mut self, sym: SymbolId, ctx: Ctx) -> bool {
        let flags = self.u.symbol(sym).flags;
        let private = flags.has(PRIVATE) || flags.has(PRIVATEqualified);
        let protected = flags.has(PROTECTED) || flags.has(PROTECTEDqualified);
        if !private && !protected {
            return true;
        }
        let Some(owner) = self.u.symbol(sym).owner else { return true };
        let mut encl = Some(ctx.owner);
        while let Some(e) = encl {
            if e == owner || protected && self.u.derives_from(e, owner) {
                return true;
            }
            encl = self.u.symbol(e).owner;
        }
        false
    }
}
