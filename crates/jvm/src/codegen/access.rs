// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Field accesses and calls with their receivers (dotty's `genLoadQualifier`, `genApply`,
//! `genCallMethod`).

use super::body::Body;
use super::callee::{Receiver, adapt_phantom};
use super::descriptors::{class_internal, method_descriptor, with_receiver};
use super::{Gen, is_static_member, method_name, unsupported};
use classfile::{FieldOp, Invoke};
use lower::Result;
use syntax::ast::Span;
use tasty::tags::PRIVATE;
use tasty::{SymbolId, name_kinds, nme};
use typer::tpd::{List, TNodeId, Tree};

impl Gen<'_, '_> {
    /// Loads field `f` of `qualifier`, or of the class `prefix` of an identifier without one, or
    /// stores the right-hand side of assignment `value` into it; returns the slots loaded.
    pub fn field(
        &mut self,
        b: &mut Body,
        f: SymbolId,
        (qualifier, prefix): (Option<TNodeId>, Option<SymbolId>),
        value: Option<(TNodeId, TNodeId)>,
    ) -> Result<i32> {
        let owner = self.l.u.symbol(f).owner.expect("a field's owner");
        let desc = self.storage_descriptor(f);
        let instance = self.receiver(b, (f, false), qualifier, prefix)?;
        let op = match (instance, value) {
            (true, None) => FieldOp::GetField,
            (false, None) => FieldOp::GetStatic,
            (true, Some(_)) => FieldOp::PutField,
            (false, Some(_)) => FieldOp::PutStatic,
        };
        if let Some((v, assign)) = value {
            self.expr_as(b, v, classfile::value_slots(&desc))?;
            self.cast_to_null(b, v, &desc);
            self.line(b, assign);
        }
        let (owner, name) = (self.member_owner(owner).0, tasty::encode(&self.l.u.symbol(f).name));
        b.code.field(op, b.pool, &owner, &name, &desc);
        Ok(if value.is_some() { 0 } else { classfile::value_slots(&desc) })
    }

    /// Loads the receiver of instance member `m` selected from `qualifier`, or of an identifier
    /// without one whose prefix is `this` of or object class `prefix`; returns whether `m` is an
    /// instance member. The qualifier of a static member must be an object or `this`, whose load
    /// is elided, as that of a member of an object standing for a JDK class's static members.
    /// A value `m` that is `called` is its getter's call.
    pub fn receiver(
        &mut self,
        b: &mut Body,
        (m, called): (SymbolId, bool),
        qualifier: Option<TNodeId>,
        prefix: Option<SymbolId>,
    ) -> Result<bool> {
        let getter = called && self.l.u.symbol(m).kind == tasty::Kind::Val;
        let owner = self.l.u.symbol(m).owner.expect("a member's owner");
        let jdk = lower::is_jdk_object(self.l.u, owner);
        let static_ = jdk || (is_static_member(self.l, m) && !getter);
        let span = qualifier.map_or(Span::NONE, |q| self.l.tpd.span(q));
        if jdk && !called && !self.jdk_static_field(m, span)? {
            return unsupported(span, "a JDK static method as a field");
        }
        match qualifier {
            Some(q) if static_ => {
                let pure = matches!(self.l.tpd.tree(q), Tree::This { .. })
                    || self.l.tpd.sym(q).is_some_and(|s| self.l.u.is_module(s) || self.l.u.is_class(s));
                if !pure {
                    return unsupported(self.l.tpd.span(q), "a static member of a qualifier with effects");
                }
            }
            Some(q) => _ = self.expr(b, q)?,
            None if static_ => {}
            None => match prefix {
                Some(c) if self.l.u.is_module_class(c) => _ = self.load_module(b, c, span)?,
                Some(c) if b.this_class.as_deref() == Some(&self.class_name(c)) => _ = self.load_this(b),
                Some(c) if (b.this_class.is_some() || b.module.is_some()) && self.in_subclass_of(b, c) => {
                    _ = self.load_this(b)
                }
                _ => return unsupported(span, "a member of an enclosing instance on the JVM"),
            },
        }
        Ok(!static_)
    }

    /// Calls `fun` with `args`: a constructor of a new instance, a superclass method or a method
    /// of a receiver; returns the slots of the result.
    pub fn apply(&mut self, b: &mut Body, mut fun: TNodeId, args: List) -> Result<i32> {
        while let Tree::TypeApply { fun: f, .. } = *self.l.tpd.tree(fun) {
            fun = f;
        }
        let span = self.l.tpd.span(fun);
        let Some(m) = self.l.tpd.sym(fun) else { return unsupported(span, "a call of a function value") };
        let arg_list = self.l.tpd.items(args).to_vec();
        if self.l.is_obj_cas(m) {
            return self.var_handle_cas(b, &arg_list);
        }
        if let Some(slots) = self.boxing(b, m, &arg_list)? {
            return Ok(slots);
        }
        let owner = self.l.u.symbol(m).owner.expect("a method's owner");
        let qualifier = match *self.l.tpd.tree(fun) {
            Tree::Select { qualifier, .. } => Some(qualifier),
            Tree::Ident { .. } => None,
            _ => return unsupported(span, "a call of this form"),
        };
        if lower::is_jdk_object(self.l.u, owner) && self.jdk_static_field(m, span)? {
            return self.field(b, m, (qualifier, None), None);
        }
        self.check_call(m, owner, span)?;
        let desc = method_descriptor(self.l.u, m);
        let (mut owner_name, jdk_interface) = self.member_owner(owner);
        let prefix = if qualifier.is_none() { self.ident_prefix(fun) } else { None };
        let mut result = classfile::method_slots(&desc).1;
        let mut name = method_name(self.l.u, m);
        let (mut call_desc, mut interface) = (desc.clone(), false);
        let super_receiver = self.super_receiver(qualifier, m, owner);
        let kind = match qualifier.map(|q| self.l.tpd.tree(q).clone()) {
            Some(Tree::New { tpt }) => {
                let tpe = self.l.tpd.tpe(tpt);
                owner_name = match self.l.u.class_symbol(tpe) {
                    Some(c) => self.class_name(c),
                    None => class_internal(&self.l.u.erase(tpe)),
                };
                b.push("L");
                b.code.new_object(b.pool, &owner_name);
                b.push("L");
                b.code.dup();
                result = 1;
                Invoke::Special
            }
            Some(Tree::Super { .. }) if self.java_super_target(fun).is_some() => {
                (owner_name, interface) = (self.class_name(super_receiver), true);
                self.load_this(b);
                Invoke::Special
            }
            Some(Tree::Super { .. }) if self.l.u.is_trait(super_receiver) => {
                if !self.l.u.is_trait(owner) || self.is_java_class(owner) {
                    return unsupported(
                        span,
                        "a super call of a Java or class method via a trait on the JVM",
                    );
                }
                owner_name = self.class_name(owner);
                self.load_this(b);
                call_desc = with_receiver(&self.class_name(owner), &desc);
                if name != nme::TRAIT_CONSTRUCTOR {
                    name = name_kinds::trait_super_accessor(&name);
                }
                interface = true;
                Invoke::Static
            }
            Some(Tree::Super { .. }) => {
                owner_name = self.class_name(super_receiver);
                self.load_this(b);
                Invoke::Special
            }
            _ if !self.receiver(b, (m, true), qualifier, prefix)? => Invoke::Static,
            _ if self.l.u.symbol(m).flags.has(PRIVATE) || self.l.u.is_constructor(m) => Invoke::Special,
            _ => {
                let receiver = match qualifier {
                    Some(q) => self.receiver_class(q, m),
                    None => prefix.map(Receiver::Class),
                };
                let receiver = receiver.unwrap_or(Receiver::Class(owner));
                let kind;
                (kind, owner_name) = match receiver {
                    Receiver::Class(c) if self.l.u.is_trait(c) => (Invoke::Interface, self.class_name(c)),
                    Receiver::Class(c) => (Invoke::Virtual, self.class_name(c)),
                    Receiver::Array(name) => (Invoke::Virtual, name),
                };
                kind
            }
        };
        let (params, _) = classfile::split_method(&desc);
        if arg_list.len() != params.len() {
            return unsupported(span, "arguments other than the descriptor's on the JVM");
        }
        for (a, param) in arg_list.into_iter().zip(params) {
            if self.expr(b, a)? != classfile::value_slots(param) {
                return unsupported(self.l.tpd.span(a), "an argument of another JVM type");
            }
            self.cast_to_null(b, a, param);
        }
        let in_trait = self.l.u.is_trait(owner) && owner_name == self.class_name(owner);
        let interface = interface || kind == Invoke::Interface || in_trait || jdk_interface;
        b.code.invoke_in(kind, interface, b.pool, (&owner_name, &name, &call_desc));
        adapt_phantom(b, classfile::split_method(&call_desc).1);
        Ok(result)
    }
}
