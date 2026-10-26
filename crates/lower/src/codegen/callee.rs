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

/// Scala Native's methods whose body is its `intrinsic` marker, generated inline by nscplugin
/// (`NirPrimitives`): owner and method name, besides `Intrinsics` and the `CFuncPtr`s.
const INTRINSICS: [(&str, &str); 2] = [
    ("scala.scalanative.unsafe.package$", "resolved"),
    ("scala.scalanative.runtime.SafeZoneAllocator$", "allocate"),
];

/// Whether method `name` of class `owner`, full names, is a Scala Native intrinsic.
fn is_intrinsic(owner: &str, name: &str) -> bool {
    let func = owner.strip_prefix("scala.scalanative.unsafe.CFuncPtr").map(|n| match n.strip_suffix('$') {
        Some(n) => (n, "fromScalaFunction"),
        None => (n, "apply"),
    });
    let plain = ["multiplyHigh", "unsignedMultiplyHigh"].contains(&name);
    (owner.starts_with(super::intrinsics::INTRINSICS) && !plain)
        || func.is_some_and(|(n, m)| n.parse::<u8>().is_ok() && m == name)
        || INTRINSICS.contains(&(owner, name))
}

impl Gen<'_, '_> {
    /// Fails for a method the lowering cannot call yet, extern methods included.
    pub fn check_callee(&mut self, sym: SymbolId, span: Span) -> Result<()> {
        self.check_call(sym, span)?;
        if is_extern(self.l.u, sym) {
            return unsupported(span, "native interop");
        }
        Ok(())
    }

    /// Fails for a method the lowering cannot call yet, extern methods excepted.
    pub fn check_call(&mut self, sym: SymbolId, span: Span) -> Result<()> {
        if let Some(owner) = self.l.u.symbol(sym).owner {
            let owner = self.l.u.full_name(owner);
            let name = &self.l.u.symbol(sym).name;
            if is_intrinsic(&owner, name) {
                let class = owner.rsplit('.').next().unwrap_or(&owner);
                return unsupported(span, format!("intrinsic {class}.{name}"));
            }
        }
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
