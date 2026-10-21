// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Checks of references: access, values, `Local` members.

use super::Typer;
use crate::ctx::Ctx;
use syntax::ast::flags as f;
use tasty::tags::PRIVATE;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Checks a reference to `sym` through `prefix` from `ctx`: accessible, and inside the subset
    /// as far as inlining goes.
    pub fn check_reference(
        &mut self,
        sym: SymbolId,
        prefix: Option<TypeId>,
        ctx: Ctx,
    ) -> Result<(), &'static str> {
        if !self.is_accessible_from(sym, prefix, ctx) {
            return Err("inaccessible member");
        }
        self.check_inline_reference(sym, ctx)?;
        self.check_self_members(sym, prefix)?;
        self.unset_local(sym, prefix)
    }

    /// Checks that term `name` for `sym`, neither a qualifier nor applied to (type) arguments, is a
    /// value: no package, nor a Java class's object (dotty's `checkValue`).
    pub fn check_value(
        &mut self,
        sym: SymbolId,
        name: syntax::ast::TName,
        applied: bool,
        ctx: Ctx,
    ) -> Result<(), &'static str> {
        if name.is_type || applied || ctx.mode.is(crate::ctx::Mode::QUALIFIER) {
            return Ok(());
        }
        if self.u.symbol(sym).kind == tasty::Kind::Package {
            return Err("package used as a value");
        }
        if self.u.symbol(sym).flags.has(tasty::tags::OBJECT) && self.is_java_class_name(sym) {
            return Err("object of a Java class used as a value");
        }
        Ok(())
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
