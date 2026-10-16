//! Whether a member is accessible (dotty's `isAccessibleFrom`, `accessBoundary`,
//! `enclosingClassNamed`).

use super::Typer;
use crate::ctx::Ctx;
use tasty::tags::{PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether `sym` may be referred to from `ctx` through some prefix.
    pub fn is_accessible(&mut self, sym: SymbolId, ctx: Ctx) -> bool {
        self.is_accessible_from(sym, None, ctx)
    }

    /// Whether `sym` may be referred to from `ctx` through `prefix`, a `this` prefix if `None`
    /// (dotty's `isAccessibleFrom`, its `Local` rules in `unset_local`).
    pub fn is_accessible_from(&mut self, sym: SymbolId, prefix: Option<TypeId>, ctx: Ctx) -> bool {
        if self.is_within_boundary(sym, ctx) {
            return true;
        }
        let flags = self.u.symbol(sym).flags;
        if !(flags.has(PROTECTED) || flags.has(PROTECTEDqualified)) {
            return false;
        }
        match prefix.map(|p| self.u.types().get(p).clone()) {
            None | Some(Type::This(_) | Type::Super(..)) => self.enclosing_sub_class(sym, ctx).is_some(),
            Some(_) => self.is_protected_access_ok(sym, prefix, ctx),
        }
    }

    /// Whether `sym` is public or `ctx` is inside its access boundary or the boundary's companion.
    pub fn is_within_boundary(&mut self, sym: SymbolId, ctx: Ctx) -> bool {
        let Some(boundary) = self.access_boundary(sym) else { return true };
        if self.u.symbol(boundary).kind.is_term() || self.access_within(boundary, ctx) {
            return true;
        }
        self.linked_class(boundary).is_some_and(|linked| self.access_within(linked, ctx))
    }

    /// The class, package or term up to which `sym` is accessible, `None` if it is public
    /// (dotty's `accessBoundary(owner)`).
    fn access_boundary(&mut self, sym: SymbolId) -> Option<SymbolId> {
        let s = self.u.symbol(sym);
        let owner = s.owner?;
        if s.flags.has(PRIVATE) {
            return Some(owner);
        }
        if let Some(&within) = self.syms.private_within.get(&sym) {
            return Some(within);
        }
        let restricted = [PRIVATEqualified, PROTECTED, PROTECTEDqualified].iter().any(|&t| s.flags.has(t));
        restricted.then_some(owner)
    }

    /// Whether `ctx`'s owner is `boundary` or inside it (dotty's `accessWithin`).
    fn access_within(&self, boundary: SymbolId, ctx: Ctx) -> bool {
        let mut o = Some(ctx.owner);
        while let Some(s) = o {
            if s == boundary {
                return true;
            }
            o = self.u.symbol(s).owner;
        }
        false
    }

    /// The companion of class `c`: the class of its object, or the class of the object it is,
    /// in the same scope if `c` is local (dotty's `linkedClass`).
    fn linked_class(&mut self, c: SymbolId) -> Option<SymbolId> {
        if !self.u.is_class(c) {
            return None;
        }
        let module_class = self.u.is_module_class(c);
        let owner = self.u.symbol(c).owner?;
        if !matches!(self.u.symbol(owner).kind, tasty::Kind::Class | tasty::Kind::Package) {
            let name = &self.u.symbol(c).name;
            let name = match module_class {
                true => tasty::module_name(name)?.to_owned(),
                false => tasty::module_class_name(name),
            };
            let u = &*self.u;
            return self
                .levels
                .scope_mate(c, &name, |s| u.is_class(s) && u.is_module_class(s) != module_class);
        }
        if module_class {
            return self.u.companion_class(c);
        }
        let module = self.u.companion_module(c)?;
        self.u.module_class(module)
    }

    /// The innermost class enclosing `ctx` that derives from `sym`'s owner (dotty's
    /// `enclosingSubClass`).
    fn enclosing_sub_class(&mut self, sym: SymbolId, ctx: Ctx) -> Option<SymbolId> {
        let owner = self.u.symbol(sym).owner?;
        let mut o = Some(ctx.owner);
        while let Some(s) = o {
            if self.u.is_class(s) && self.u.derives_from(s, owner) {
                return Some(s);
            }
            o = self.u.symbol(s).owner;
        }
        None
    }

    /// Whether protected `sym` may be selected from `prefix` in `ctx` (dotty's
    /// `isProtectedAccessOK`, package objects left out).
    fn is_protected_access_ok(&mut self, sym: SymbolId, prefix: Option<TypeId>, ctx: Ctx) -> bool {
        let Some(cls) = self.enclosing_sub_class(sym, ctx) else { return false };
        let s = self.u.symbol(sym);
        if s.kind.is_type() || s.owner.is_some_and(|o| self.u.is_module_class(o)) {
            return true;
        }
        let constructor_in_super_call = s.name == "<init>" && self.super_call_class(ctx).is_some();
        let pre = prefix.map(|p| self.u.widen(p)).and_then(|p| self.u.class_symbol(p));
        constructor_in_super_call || pre.is_some_and(|p| self.u.derives_from(p, cls))
    }

    /// The class whose primary constructor owns `ctx`, the context of its parent calls (dotty's
    /// `isSuperCallContext`).
    fn super_call_class(&self, ctx: Ctx) -> Option<SymbolId> {
        let c = self.u.symbol(ctx.owner).owner?;
        (self.syms.primary_ctors.get(&c) == Some(&ctx.owner)).then_some(c)
    }

    /// Refuses protected `sym` selected from a path of type `qual_tpe` other than `this` or
    /// `super`, outside its access boundary, unless it is a constructor of a parent of the class
    /// whose parent calls `ctx` is in and `new` the qualifier.
    pub fn check_protected_path(
        &mut self,
        sym: SymbolId,
        qual_tpe: TypeId,
        new: bool,
        ctx: Ctx,
    ) -> Result<(), &'static str> {
        let flags = self.u.symbol(sym).flags;
        if !(flags.has(PROTECTED) || flags.has(PROTECTEDqualified))
            || matches!(self.u.types().get(qual_tpe), Type::This(_) | Type::Super(..))
            || self.is_within_boundary(sym, ctx)
        {
            return Ok(());
        }
        let s = self.u.symbol(sym);
        let parent = s.owner;
        let super_call = new
            && s.name == "<init>"
            && self.super_call_class(ctx).zip(parent).is_some_and(|(c, p)| self.u.derives_from(c, p));
        if super_call { Ok(()) } else { Err("protected member through a path") }
    }

    /// The class or package enclosing `owner`, or `owner` itself, whose name or object's name
    /// is `name` (dotty's `enclosingClassNamed`).
    pub fn enclosing_class_named(&self, owner: SymbolId, name: &str) -> Option<SymbolId> {
        let mut o = Some(owner);
        while let Some(s) = o {
            let sym = self.u.symbol(s);
            let named =
                sym.name == name || self.u.is_module_class(s) && tasty::module_name(&sym.name) == Some(name);
            if named && matches!(sym.kind, tasty::Kind::Class | tasty::Kind::Package) {
                return Some(s);
            }
            o = sym.owner;
        }
        None
    }
}
