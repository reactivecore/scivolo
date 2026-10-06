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
        self.unset_local(sym, prefix)
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

    /// Drops `Local` from member `sym` accessed through `prefix`, not its owner's `this`, if it
    /// may lose it (dotty's `canBeLocal`): a script member other than a parameter or its accessor.
    fn unset_local(&mut self, sym: SymbolId, prefix: Option<TypeId>) -> Result<(), &'static str> {
        let (Some(prefix), Some(owner)) = (prefix, self.u.symbol(sym).owner) else { return Ok(()) };
        if !self.u.symbol(sym).flags.has(tasty::tags::LOCAL) || self.is_privileged_prefix(prefix, owner) {
            return Ok(());
        }
        let flags = self.syms.flags.get(&sym).copied();
        match flags {
            Some(fl) if !fl.is(f::PARAM) && !fl.is(f::PARAM_ACCESSOR) => {
                self.syms.set_flags(self.u, sym, fl.and_not(f::LOCAL));
                Ok(())
            }
            _ => Err("inaccessible member"),
        }
    }

    /// Whether type `t` refers to a private type member of a class other than the class
    /// enclosing `ctx`.
    pub fn mentions_foreign_private_type(&mut self, t: TypeId, ctx: Ctx) -> bool {
        let here = self.enclosing_class(ctx.owner);
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            if let Type::Sym(_, s) = *u.types().get(part) {
                let sym = u.symbol(s);
                found |= sym.kind == tasty::Kind::Type && sym.flags.has(PRIVATE) && sym.owner != here;
            }
            None
        });
        found
    }

    /// Whether `prefix` is the `this` of class `owner` or the object whose class it is (dotty's
    /// `isAccessPrivilegedThisType`).
    fn is_privileged_prefix(&mut self, prefix: TypeId, owner: SymbolId) -> bool {
        match self.u.types().get(prefix).clone() {
            Type::This(c) => self.u.class_symbol(c) == Some(owner),
            Type::Sym(_, s) => self.u.module_class(s) == Some(owner),
            _ => false,
        }
    }
}
