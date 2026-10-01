//! Access to private and protected members (dotty's `isAccessibleFrom`, as far as the subset
//! goes: a restricted member is accessible from inside its class).

use super::Typer;
use crate::ctx::Ctx;
use syntax::ast::flags as f;
use tasty::tags::{PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Checks a reference to `sym` through `prefix` from `ctx`: accessible, and not to an inline
    /// method, whose calls are expanded (a later step).
    pub fn check_reference(
        &mut self,
        sym: SymbolId,
        prefix: Option<TypeId>,
        ctx: Ctx,
    ) -> Result<(), &'static str> {
        if !self.is_accessible(sym, ctx) {
            return Err("inaccessible member");
        }
        if self.u.symbol(sym).flags.has(tasty::tags::INLINE) {
            return Err("inline method call");
        }
        self.unset_local(sym, prefix);
        Ok(())
    }

    /// Whether `sym` may be referred to from `ctx`: it is unrestricted, or `ctx` is inside its
    /// owner, or, if it is protected, inside a subclass of its owner.
    pub fn is_accessible(&mut self, sym: SymbolId, ctx: Ctx) -> bool {
        let flags = self.u.symbol(sym).flags;
        let private = flags.has(PRIVATE) || flags.has(PRIVATEqualified);
        let protected = flags.has(PROTECTED) || flags.has(PROTECTEDqualified);
        let within = self.syms.private_within.get(&sym).copied();
        if !private && !protected && within.is_none() {
            return true;
        }
        let Some(owner) = within.or(self.u.symbol(sym).owner) else { return true };
        let mut encl = Some(ctx.owner);
        while let Some(e) = encl {
            if e == owner || protected && self.u.derives_from(e, owner) {
                return true;
            }
            encl = self.u.symbol(e).owner;
        }
        false
    }

    /// Drops `Local` from script member `sym` accessed through `prefix`, not its owner's `this`.
    fn unset_local(&mut self, sym: SymbolId, prefix: Option<TypeId>) {
        let Some(&flags) = self.syms.flags.get(&sym) else { return };
        let (Some(prefix), Some(owner)) = (prefix, self.u.symbol(sym).owner) else { return };
        if !flags.is(f::LOCAL) || self.is_privileged_prefix(prefix, owner) {
            return;
        }
        self.syms.set_flags(self.u, sym, flags.and_not(f::LOCAL));
    }

    /// Whether `prefix` is the `this` of class `owner` or the object whose class it is (dotty's
    /// `isAccessPrivilegedThisType`).
    fn is_privileged_prefix(&mut self, prefix: TypeId, owner: SymbolId) -> bool {
        match self.u.types().get(prefix).clone() {
            Type::This(c) => self.u.class_symbol(c) == Some(owner),
            Type::Sym(_, s) => self.u.module_class(s) == Some(owner),
            Type::Package(_) => true,
            _ => false,
        }
    }
}
