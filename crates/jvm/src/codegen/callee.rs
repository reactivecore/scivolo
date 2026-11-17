// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! What the JVM backend can call (dotty's `genApply` and `genCallMethod`).

use super::body::Body;
use super::descriptors::{NOTHING, class_descriptor};
use super::{Gen, unsupported};
use classfile::{ATHROW, FieldOp};
use lower::Result;
use names::nme;
use syntax::ast::Span;
use tasty::tags::{INLINE, MACRO};
use tasty::{SymbolId, Type};
use typer::tpd::{List, TNodeId, Tree};

/// The class or array type a virtual call names.
pub enum Receiver {
    Class(SymbolId),
    Array(String),
}

/// After a call returning `Nothing$` an `athrow`, after one returning `Null$` a `null` in place
/// of its value (dotty's `adapt`).
pub fn adapt_phantom(b: &mut Body, result: &str) {
    match result {
        NOTHING => b.code.insn(ATHROW),
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
        if self.is_throw(t)
            && let &[e] = self.l.tpd.items(args)
        {
            let nothing = self.desc_of(e) == NOTHING;
            self.expr_to(b, e, 1, super::body::Dest::Throw(nothing))?;
            return Ok(0);
        }
        if let Some(slots) = self.hash(b, t)? {
            return Ok(slots);
        }
        self.apply(b, fun, args)
    }

    /// Whether `t` is a call of `throw`.
    pub fn is_throw(&self, t: TNodeId) -> bool {
        let Tree::Apply { fun, .. } = *self.l.tpd.tree(t) else { return false };
        let Some(s) = self.l.tpd.sym(fun).map(|m| self.l.u.symbol(m)) else { return false };
        s.name == nme::THROW && s.owner.is_some_and(|o| self.l.u.symbol(o).name == nme::OPS_PACKAGE)
    }

    /// Fails for method `m` of class `owner` that the backend cannot call yet: the primitive
    /// operations, array operations and `throw` dotty generates inline that `primitive`,
    /// `array_op` and `boxing` leave, and unexpanded inline methods.
    pub fn check_call(&mut self, m: SymbolId, owner: SymbolId, span: Span) -> Result<()> {
        let name = self.l.u.symbol(m).name.clone();
        let owner_name = self.l.u.binary_name(owner);
        let object_ops = [nme::REF_EQ, nme::REF_NE, nme::SYNCHRONIZED, nme::EQ, nme::NE, nme::HASHHASH];
        let any_ops = [nme::EQ, nme::NE, nme::HASHHASH, nme::IS_INSTANCE_OF, nme::AS_INSTANCE_OF];
        let primitive = (owner_name == "scala.Any" && any_ops.contains(&name.as_str()))
            || (owner_name == "java.lang.Object" && object_ops.contains(&name.as_str()))
            || (owner_name == "java.lang.String" && name == nme::ADD)
            || is_primitive(&owner_name)
            || owner_name == "scala.Array"
            || self.l.u.symbol(owner).name == nme::OPS_PACKAGE;
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
        let p = match *self.l.u.ty(self.l.tpd.tpe(id)) {
            Type::Sym(Some(p), _) | Type::TermRef(p, _) => p,
            _ => return None,
        };
        match *self.l.u.ty(p) {
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

    pub fn load_this(&mut self, b: &mut Body) -> i32 {
        b.push("L");
        match (&b.this_class, &b.module) {
            (_, Some(module)) => {
                let module = module.clone();
                let desc = class_descriptor(&module);
                b.code.field(FieldOp::GetStatic, b.pool, &module, nme::MODULE_INSTANCE_FIELD, &desc);
            }
            _ => b.code.load("L", 0),
        }
        1
    }

    /// Loads object class `class`: `this` in its own instance methods (dotty's `genLoadModule`).
    pub fn load_module(&mut self, b: &mut Body, class: SymbolId, span: Span) -> Result<i32> {
        if lower::is_jdk_object(self.l.u, class) {
            return unsupported(span, "objects standing for JDK classes on the JVM");
        }
        let name = self.class_name(class);
        b.push("L");
        match b.this_class.as_deref() == Some(&name) {
            true => b.code.load("L", 0),
            false => {
                let desc = class_descriptor(&name);
                b.code.field(FieldOp::GetStatic, b.pool, &name, nme::MODULE_INSTANCE_FIELD, &desc)
            }
        }
        Ok(1)
    }
}
