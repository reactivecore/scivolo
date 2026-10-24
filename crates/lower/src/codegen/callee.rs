//! What the lowering can call: the methods it refuses to call yet.

use super::Gen;
use crate::symbols::{has, is_extern};
use crate::{Result, unsupported};
use syntax::ast::Span;
use tasty::SymbolId;
use tasty::tags::{INLINE, MACRO};

/// Whether binary class name `name` is a primitive value class.
pub fn is_primitive(name: &str) -> bool {
    let prims = ["Boolean", "Char", "Byte", "Short", "Int", "Long", "Float", "Double", "Unit"];
    name.strip_prefix("scala.").is_some_and(|n| prims.contains(&n))
}

impl Gen<'_, '_> {
    /// Fails for a method the lowering cannot call yet.
    pub fn check_callee(&mut self, sym: SymbolId, span: Span) -> Result<()> {
        if !self.l.u.symbol(sym).owner.is_some_and(|o| self.l.u.is_class(o)) {
            return unsupported(span, "calls of methods outside classes");
        }
        if self.l.u.symbol(sym).owner.is_some_and(|o| self.l.u.full_name(o) == "scala.Array") {
            return unsupported(span, "array operations");
        }
        if self.is_primitive_op(sym) {
            return unsupported(span, "primitive operations");
        }
        if has(self.l.u, sym, INLINE) || has(self.l.u, sym, MACRO) {
            return unsupported(span, "inline call not expanded");
        }
        if is_extern(self.l.u, sym) {
            return unsupported(span, "native interop");
        }
        if self.l.u.full_name(sym) == "scala.scalanative.unsafe.package$.extern" {
            return unsupported(span, "`extern` outside an extern object");
        }
        Ok(())
    }

    /// Whether `sym` is dotty's `<special-ops>.throw`.
    pub fn is_throw(&self, sym: SymbolId) -> bool {
        let s = self.l.u.symbol(sym);
        s.name == "throw" && s.owner.is_some_and(|o| self.l.u.symbol(o).name == "<special-ops>")
    }

    /// Whether `sym` is an operation nscplugin generates inline: of `Any`, of a primitive class,
    /// reference equality, `synchronized`, string concatenation.
    fn is_primitive_op(&mut self, sym: SymbolId) -> bool {
        let Some(owner) = self.l.u.symbol(sym).owner else { return false };
        let name = self.l.u.symbol(sym).name.as_str();
        let owner_name = self.l.u.binary_name(owner);
        let object_ops = ["eq", "ne", "synchronized", "==", "!=", "##"];
        let any_ops = ["==", "!=", "##", "isInstanceOf", "asInstanceOf"];
        (owner == self.l.defs.any && any_ops.contains(&name))
            || is_primitive(&owner_name)
            || (owner == self.l.defs.object && object_ops.contains(&name))
            || (owner_name == "java.lang.String" && name == "+")
    }
}
