// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! What the JVM backend can call, and the class a call names (dotty's `genApply` and
//! `genCallMethod`).

use super::body::Body;
use super::{Gen, descriptor, unsupported};
use lower::Result;
use syntax::ast::Span;
use tasty::tags::{INLINE, MACRO};
use tasty::{Erased, SymbolId, Type};
use typer::tpd::{List, TNodeId, Tree};

/// The class or array type a virtual call names.
pub enum Receiver {
    Class(SymbolId),
    Array(String),
}

/// `Object`'s methods a trait may override.
const OBJECT_METHODS: [&str; 5] = ["equals", "hashCode", "toString", "clone", "finalize"];

/// After a call returning `Nothing$` an `athrow`, after one returning `Null$` a `null` in place
/// of its value (dotty's `adapt`).
pub fn adapt_phantom(b: &mut Body, result: &str) {
    match result {
        "Lscala/runtime/Nothing$;" => b.code.insn(0xbf),
        "Lscala/runtime/Null$;" => {
            b.code.pop();
            b.code.aconst_null();
        }
        _ => {}
    }
}

/// Whether binary class name `name` is a primitive value class.
fn is_primitive(name: &str) -> bool {
    let prims = ["Boolean", "Char", "Byte", "Short", "Int", "Long", "Float", "Double", "Unit"];
    name.strip_prefix("scala.").is_some_and(|n| prims.contains(&n))
}

impl Gen<'_, '_> {
    /// Call `t` of `fun` with `args`: a primitive operation, an array operation, a string
    /// concatenation, `throw`, a hash, or a call of a method.
    pub fn gen_apply(&mut self, b: &mut Body, t: TNodeId, fun: TNodeId, args: List) -> Result<i32> {
        if let Some(slots) = self.primitive(b, t)? {
            return Ok(slots);
        }
        if let Some(slots) = self.array_op(b, t, fun, args)? {
            return Ok(slots);
        }
        if self.is_concat(t) {
            return self.concat(b, t);
        }
        let m = self.l.tpd.sym(fun);
        let s = m.map(|m| self.l.u.symbol(m));
        let special = s.and_then(|s| s.owner).is_some_and(|o| self.l.u.symbol(o).name == "<special-ops>");
        if special
            && s.is_some_and(|s| s.name == "throw")
            && let &[e] = self.l.tpd.items(args)
        {
            self.expr(b, e)?;
            b.code.insn(0xbf);
            return Ok(0);
        }
        if let Some(slots) = self.hash(b, t)? {
            return Ok(slots);
        }
        self.apply(b, fun, args)
    }

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

    /// The class of `this` or of the static object that identifier `id`'s prefix stands for
    /// (dotty's `desugarIdent`).
    pub fn ident_prefix(&mut self, id: TNodeId) -> Option<SymbolId> {
        let p = match *self.l.u.types().get(self.l.tpd.tpe(id)) {
            Type::Sym(Some(p), _) | Type::TermRef(p, _) => p,
            _ => return None,
        };
        match *self.l.u.types().get(p) {
            Type::This(c) => self.l.u.class_symbol(c),
            Type::Sym(_, s) if self.l.u.is_module(s) && self.l.u.is_static_symbol(s) => {
                self.l.u.module_class(s)
            }
            _ => None,
        }
    }

    /// Whether the class of the code being generated derives from class `c`.
    pub fn in_subclass_of(&mut self, b: &Body, c: SymbolId) -> bool {
        let named = b.this_class.as_ref().or(b.module.as_ref());
        let class = b.method.and_then(|m| self.l.u.symbol(m).owner);
        let class = class.or_else(|| named.and_then(|n| self.classes.get(n).copied().flatten()));
        class.is_some_and(|o| self.l.u.is_class(o) && self.l.u.derives_from(o, c))
    }

    /// The class a virtual call of `m` on `qualifier` names: the qualifier's erased class (its
    /// class for `super`), `Object` or the array type for an array, `m`'s trait for a trait method
    /// overriding one of `Object`; none where the qualifier's type erases to no class of its own or
    /// to one not deriving from `m`'s owner, as a by-name parameter's.
    pub fn receiver_class(&mut self, qualifier: TNodeId, m: SymbolId) -> Option<Receiver> {
        let owner = self.l.u.symbol(m).owner?;
        let t = self.l.tpd.tpe(qualifier);
        let t = self.l.u.widen(t);
        let erased = match self.l.tpd.tree(qualifier) {
            Tree::Super { .. } => self.l.u.erase(t),
            _ => lower::erased_type(self.l, qualifier),
        };
        match erased {
            e @ Erased::Array(_) if self.l.u.symbol(m).name == "clone" => {
                Some(Receiver::Array(descriptor(&e)))
            }
            Erased::Array(_) => Some(Receiver::Array("java/lang/Object".into())),
            Erased::Class(name) => {
                let c = self.l.u.class_symbol(t)?;
                let c = match self.l.u.binary_name(c) == name {
                    true => c,
                    false => {
                        self.l.u.linearization(c).into_iter().find(|&b| self.l.u.binary_name(b) == name)?
                    }
                };
                if !self.l.u.derives_from(c, owner) {
                    return None;
                }
                let object_method = OBJECT_METHODS.contains(&self.l.u.symbol(m).name.as_str());
                let trait_override = c != owner && self.l.u.is_trait(c) && self.l.u.is_trait(owner);
                Some(Receiver::Class(if trait_override && object_method { owner } else { c }))
            }
            _ => None,
        }
    }
}
