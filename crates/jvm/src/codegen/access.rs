// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Field accesses and calls with their receivers (dotty's `genLoadQualifier`, `genApply`,
//! `genCallMethod`).

use super::method::Body;
use super::{Gen, class_internal, descriptor, is_static_member, method_descriptor, method_name, unsupported};
use classfile::{FieldOp, Invoke};
use lower::Result;
use tasty::SymbolId;
use tasty::tags::PRIVATE;
use typer::tpd::{List, TNodeId, Tree};

impl Gen<'_, '_> {
    /// Loads field `f` of `qualifier`, or stores `value` into it; returns the slots loaded.
    pub fn field(
        &mut self,
        b: &mut Body,
        f: SymbolId,
        qualifier: TNodeId,
        value: Option<TNodeId>,
    ) -> Result<i32> {
        let owner = self.l.u.symbol(f).owner.expect("a field's owner");
        let info = self.l.u.info(f);
        let desc = descriptor(&self.l.u.erase(info));
        let instance = self.receiver(b, f, Some(qualifier))?;
        let op = match (instance, value) {
            (true, None) => FieldOp::GetField,
            (false, None) => FieldOp::GetStatic,
            (true, Some(_)) => FieldOp::PutField,
            (false, Some(_)) => FieldOp::PutStatic,
        };
        if let Some(v) = value {
            self.expr(b, v)?;
        }
        let (owner, name) = (self.class_name(owner), tasty::encode(&self.l.u.symbol(f).name));
        b.code.field(op, b.pool, &owner, &name, &desc);
        Ok(if value.is_some() { 0 } else { classfile::value_slots(&desc) })
    }

    /// Loads the receiver of instance member `m` selected from `qualifier`, or of an identifier
    /// without one; returns whether `m` is an instance member. The qualifier of a static member
    /// must be an object or `this`, whose load is elided.
    fn receiver(&mut self, b: &mut Body, m: SymbolId, qualifier: Option<TNodeId>) -> Result<bool> {
        let static_ = is_static_member(self.l.u, m);
        let owner = self.l.u.symbol(m).owner.expect("a member's owner");
        match qualifier {
            Some(q) if static_ => {
                let pure = matches!(self.l.tpd.tree(q), Tree::This { .. })
                    || self.l.tpd.sym(q).is_some_and(|s| self.l.u.is_module(s));
                if !pure {
                    return unsupported(self.l.tpd.span(q), "a static member of a qualifier with effects");
                }
            }
            Some(q) => _ = self.expr(b, q)?,
            None if static_ => {}
            None if self.l.u.is_module_class(owner) => _ = self.load_module(b, owner),
            None => _ = self.load_this(b),
        }
        Ok(!static_)
    }

    /// Calls `fun` with `args`: a constructor of a new instance, a superclass method or a method
    /// of a receiver; returns the slots of the result.
    pub fn apply(&mut self, b: &mut Body, fun: TNodeId, args: List) -> Result<i32> {
        let span = self.l.tpd.span(fun);
        let Some(m) = self.l.tpd.sym(fun) else { return unsupported(span, "a call of a function value") };
        let desc = method_descriptor(self.l.u, m);
        let owner = self.l.u.symbol(m).owner.expect("a method's owner");
        let mut owner_name = self.class_name(owner);
        let qualifier = match *self.l.tpd.tree(fun) {
            Tree::Select { qualifier, .. } => Some(qualifier),
            Tree::Ident { .. } => None,
            _ => return unsupported(span, "a call of this form"),
        };
        let (mut kind, mut result) = (Invoke::Virtual, classfile::method_slots(&desc).1);
        match qualifier.map(|q| self.l.tpd.tree(q).clone()) {
            Some(Tree::New { tpt }) => {
                owner_name = class_internal(&self.l.u.erase(self.l.tpd.tpe(tpt)));
                b.code.new_object(b.pool, &owner_name);
                b.code.dup();
                (kind, result) = (Invoke::Special, 1);
            }
            Some(Tree::Super { .. }) => {
                self.load_this(b);
                kind = Invoke::Special;
            }
            _ if !self.receiver(b, m, qualifier)? => kind = Invoke::Static,
            _ if self.l.u.symbol(m).flags.has(PRIVATE) || self.l.u.is_constructor(m) => {
                kind = Invoke::Special
            }
            _ if self.l.u.is_trait(owner) => kind = Invoke::Interface,
            _ => {}
        }
        for a in self.l.tpd.items(args).to_vec() {
            self.expr(b, a)?;
        }
        let name = method_name(self.l.u, m);
        b.code.invoke(kind, b.pool, &owner_name, &name, &desc);
        Ok(result)
    }
}
