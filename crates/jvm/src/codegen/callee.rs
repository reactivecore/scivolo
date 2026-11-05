// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! What the JVM backend can call, and the class a call names (dotty's `genApply` and
//! `genCallMethod`).

use super::{Gen, descriptor, unsupported};
use lower::Result;
use syntax::ast::Span;
use tasty::tags::{INLINE, MACRO};
use tasty::{Erased, SymbolId};
use typer::tpd::TNodeId;

/// The class or array type a virtual call names.
pub enum Receiver {
    Class(SymbolId),
    Array(String),
}

/// `Object`'s methods a trait may override.
const OBJECT_METHODS: [&str; 5] = ["equals", "hashCode", "toString", "clone", "finalize"];

/// Whether binary class name `name` is a primitive value class or its companion's class.
fn is_primitive(name: &str) -> bool {
    let prims = ["Boolean", "Char", "Byte", "Short", "Int", "Long", "Float", "Double", "Unit"];
    let name = name.strip_suffix('$').unwrap_or(name);
    name.strip_prefix("scala.").is_some_and(|n| prims.contains(&n))
}

impl Gen<'_, '_> {
    /// Fails for method `m` of class `owner` that the backend cannot call yet: the primitive
    /// operations, array operations and `throw` dotty generates inline that `primitive`,
    /// `array_op` and `boxing` leave, and unexpanded inline methods.
    pub fn check_call(&mut self, m: SymbolId, owner: SymbolId, span: Span) -> Result<()> {
        let name = self.l.u.symbol(m).name.clone();
        let owner_name = self.l.u.binary_name(owner);
        let object_ops = ["eq", "ne", "synchronized", "==", "!=", "##"];
        let any_ops = ["==", "!=", "##", "isInstanceOf", "asInstanceOf"];
        let primitive = (owner_name == "scala.Any" && any_ops.contains(&name.as_str()))
            || (owner_name == "java.lang.Object" && object_ops.contains(&name.as_str()))
            || (owner_name == "java.lang.String" && name == "+")
            || is_primitive(&owner_name)
            || owner_name == "scala.Array"
            || self.l.u.symbol(owner).name == "<special-ops>";
        if primitive {
            return unsupported(span, format!("primitive operation {name} on the JVM"));
        }
        if !self.l.u.is_class(owner) {
            return unsupported(span, "calls of methods outside classes");
        }
        let flags = self.l.u.symbol(m).flags;
        if flags.has(INLINE) || flags.has(MACRO) {
            return unsupported(span, "inline call not expanded");
        }
        Ok(())
    }

    /// The class a virtual call of `m` on `qualifier` names: the qualifier's class, `Object` or
    /// the array type for an array, `m`'s trait for a trait method overriding one of `Object`;
    /// none where the qualifier's type erases to no class of its own.
    pub fn receiver_class(&mut self, qualifier: TNodeId, m: SymbolId) -> Option<Receiver> {
        let owner = self.l.u.symbol(m).owner?;
        let t = self.l.tpd.tpe(qualifier);
        let t = self.l.u.widen(t);
        match self.l.u.erase(t) {
            e @ Erased::Array(_) if self.l.u.symbol(m).name == "clone" => {
                Some(Receiver::Array(descriptor(&e)))
            }
            Erased::Array(_) => Some(Receiver::Array("java/lang/Object".into())),
            Erased::Class(name) => {
                let c = self.l.u.class_symbol(t).filter(|&c| self.l.u.binary_name(c) == name)?;
                let object_method = OBJECT_METHODS.contains(&self.l.u.symbol(m).name.as_str());
                let trait_override = c != owner && self.l.u.is_trait(c) && self.l.u.is_trait(owner);
                Some(Receiver::Class(if trait_override && object_method { owner } else { c }))
            }
            _ => None,
        }
    }
}
