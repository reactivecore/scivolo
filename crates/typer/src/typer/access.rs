// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Checks of references: access, values, `Local` members; the accessible members selected.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use syntax::ast::flags as f;
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

    /// Checks that term `name` for `sym`, neither a qualifier nor applied to (type) arguments by
    /// `pt`, is a value: no package, nor a Java class's object (dotty's `checkValue`).
    pub fn check_value(
        &mut self,
        sym: SymbolId,
        name: syntax::ast::TName,
        pt: Pt,
        ctx: Ctx,
    ) -> Result<(), &'static str> {
        let applied = matches!(pt, Pt::Fun(_) | Pt::PolyFun(_));
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

    /// Whether `sym` is accessible through `prefix` from `ctx` and not kept `Local` from it
    /// (dotty's `isAccessibleFrom`).
    pub fn is_selectable(&mut self, sym: SymbolId, prefix: TypeId, ctx: Ctx) -> bool {
        self.is_accessible_from(sym, Some(prefix), ctx)
            && !(self.is_foreign_local(sym, prefix) && self.keeps_local(sym))
    }

    /// Whether one of `syms`, members `name` of `qual_tpe`, or one of the non-private members
    /// `name` it inherits is accessible in `ctx`.
    pub fn has_selectable(&mut self, syms: &[SymbolId], qual_tpe: TypeId, name: &str, ctx: Ctx) -> bool {
        syms.iter().any(|&s| self.is_selectable(s, qual_tpe, ctx))
            || !self.inherited_selectable(syms, qual_tpe, name, ctx).is_empty()
    }

    /// The alternatives of `syms`, members `name` of `qual_tpe`, accessible in `ctx`, else the
    /// accessible non-private ones it inherits, else all of `syms` (dotty's `accessibleType`).
    pub fn accessible_or_inherited(
        &mut self,
        syms: Vec<SymbolId>,
        qual_tpe: TypeId,
        name: &str,
        ctx: Ctx,
    ) -> Vec<SymbolId> {
        if syms.iter().any(|&s| self.is_selectable(s, qual_tpe, ctx)) {
            return self.accessible_alternatives(syms, qual_tpe, ctx);
        }
        let inherited = self.inherited_selectable(&syms, qual_tpe, name, ctx);
        if inherited.is_empty() { syms } else { inherited }
    }

    /// The non-private members `name` of `qual_tpe` of the kind of `syms`, accessible in `ctx`.
    fn inherited_selectable(
        &mut self,
        syms: &[SymbolId],
        qual_tpe: TypeId,
        name: &str,
        ctx: Ctx,
    ) -> Vec<SymbolId> {
        let is_type = syms.first().is_some_and(|&s| self.u.is_type_symbol(s));
        let members = self.non_private_members(qual_tpe, name);
        members
            .into_iter()
            .filter(|&s| self.u.is_type_symbol(s) == is_type && self.is_selectable(s, qual_tpe, ctx))
            .collect()
    }

    /// Drops `Local` from member `sym` accessed through `prefix`, not its owner's `this`, if it
    /// may lose it.
    fn unset_local(&mut self, sym: SymbolId, prefix: Option<TypeId>) -> Result<(), &'static str> {
        if !prefix.is_some_and(|p| self.is_foreign_local(sym, p)) {
            return Ok(());
        }
        if self.keeps_local(sym) {
            return Err("inaccessible member");
        }
        let flags = self.syms.flags[&sym];
        self.syms.set_flags(self.u, sym, flags.and_not(f::LOCAL));
        Ok(())
    }

    /// Whether `Local` member `sym` is selected through `prefix`, not its owner's `this`.
    fn is_foreign_local(&mut self, sym: SymbolId, prefix: TypeId) -> bool {
        let Some(owner) = self.u.symbol(sym).owner else { return false };
        self.u.symbol(sym).flags.has(tasty::tags::LOCAL) && !self.is_privileged_prefix(prefix, owner)
    }

    /// Whether `sym` may not lose `Local` (dotty's `canBeLocal`): not a script member, or a
    /// parameter or its accessor.
    fn keeps_local(&self, sym: SymbolId) -> bool {
        self.syms.flags.get(&sym).is_none_or(|fl| fl.is(f::PARAM) || fl.is(f::PARAM_ACCESSOR))
    }

    /// Whether `prefix` is the `this` of class `owner` or the object whose class it is (dotty's
    /// `isAccessPrivilegedThisType`).
    fn is_privileged_prefix(&mut self, prefix: TypeId, owner: SymbolId) -> bool {
        match *self.u.types().get(prefix) {
            Type::This(c) => self.u.class_symbol(c) == Some(owner),
            Type::Sym(_, s) => self.u.module_class(s) == Some(owner),
            _ => false,
        }
    }
}
