// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! Calls: of methods, constructors and static methods (nscplugin's `genApply`).

use super::Gen;
use crate::phases::adapt::from_start;
use crate::symbols::{is_extern, is_java_static};
use crate::{Result, unsupported};
use nir::{Op, Type, Val, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{List, TNodeId, Tree};

/// The receiver of a call: a tree, or the prefix of an identifier's type at the identifier's
/// span.
#[derive(Clone, Copy)]
pub enum Receiver {
    Tree(TNodeId),
    Prefix(tasty::TypeId, Span),
}

impl Gen<'_, '_> {
    pub fn gen_apply(&mut self, t: TNodeId, fun: TNodeId, args: List) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let saved = self.at_span(span);
        let args = self.l.tpd.items(args).to_vec();
        let method = if let Tree::TypeApply { fun, .. } = *self.l.tpd.tree(fun) { fun } else { fun };
        let Some(sym) = self.l.tpd.sym(method) else {
            return unsupported(span, "a call without a method symbol");
        };
        if self.is_string_concat(sym) {
            let v = self.gen_string_concat(t, span)?;
            self.m.pos = saved;
            return Ok(v);
        }
        if let (true, [exception]) = (self.is_throw(sym), &args[..]) {
            let v = self.gen_expr(*exception)?;
            self.raise(v);
            self.m.pos = saved;
            return Ok(self.val(Val::Unit));
        }
        let primitive = match self.gen_primitive(t, sym, fun, &args)? {
            Some(v) => Some(v),
            None => self.gen_boxing(sym, &args, span)?,
        };
        if let Some(v) = primitive {
            self.m.pos = saved;
            return Ok(v);
        }
        let v = match *self.l.tpd.tree(fun) {
            Tree::TypeApply { .. } => return unsupported(span, "calls with type arguments"),
            Tree::Select { qualifier, .. } => match *self.l.tpd.tree(qualifier) {
                Tree::Super { .. } => {
                    self.check_callee(sym, span)?;
                    let this = self.m.this.expect("`this` of a method");
                    let args = self.gen_args(sym, &args, span)?;
                    self.call_method(sym, true, this, args)
                }
                Tree::New { tpt } if self.l.u.is_constructor(sym) => {
                    self.gen_apply_new(tpt, sym, &args, span)?
                }
                _ => self.gen_apply_method(
                    sym,
                    self.l.u.is_constructor(sym),
                    Receiver::Tree(qualifier),
                    &args,
                    span,
                )?,
            },
            Tree::Ident { .. } => {
                self.check_call(sym, span)?;
                let prefix = match *self.l.u.types().get(self.l.tpd.tpe(fun)) {
                    tasty::Type::Sym(Some(p), _) | tasty::Type::TermRef(p, _) => p,
                    _ => return unsupported(span, "a call of a local method"),
                };
                let own_this = match *self.l.u.types().get(prefix) {
                    tasty::Type::This(c) => self.l.u.class_symbol(c) == self.class,
                    _ => false,
                };
                let at = match args.first() {
                    Some(&first) if own_this && self.m.this.is_none() => from_start(self.l.tpd.span(first)),
                    _ => self.l.tpd.span(fun),
                };
                self.gen_apply_method(sym, false, Receiver::Prefix(prefix, at), &args, span)?
            }
            _ => return unsupported(span, "a call of a function value or with several argument lists"),
        };
        self.m.pos = saved;
        Ok(v)
    }

    fn gen_apply_new(&mut self, tpt: TNodeId, ctor: SymbolId, args: &[TNodeId], span: Span) -> Result<ValId> {
        self.check_callee(ctor, span)?;
        let Some(class) = self.l.u.class_symbol(self.l.tpd.tpe(tpt)) else {
            return unsupported(self.l.tpd.span(tpt), "`new` of a type that is not a class");
        };
        if self.is_struct(class) {
            return unsupported(span, "`new` of a `@struct` class");
        }
        let name = self.type_name(class);
        let alloc = self.emit(Op::Classalloc { name, zone: None });
        let args = self.gen_args(ctor, args, span)?;
        self.call_method(ctor, true, alloc, args);
        Ok(alloc)
    }

    pub fn gen_apply_method(
        &mut self,
        sym: SymbolId,
        statically: bool,
        recv: Receiver,
        args: &[TNodeId],
        span: Span,
    ) -> Result<ValId> {
        if let Some(v) = self.gen_reflective(sym, recv, args, span)? {
            return Ok(v);
        }
        self.check_call(sym, span)?;
        if self.is_extern_field(sym) {
            return self.load_extern_field(sym, args, span);
        }
        if is_java_static(self.l.u, sym) {
            let owner = match recv {
                Receiver::Tree(t) => self.l.tpd.sym(t),
                Receiver::Prefix(p, _) => self.l.u.class_symbol(p),
            };
            return self.gen_apply_static(sym, owner, args, span);
        }
        let receiver = match recv {
            Receiver::Tree(t) => self.gen_expr(t)?,
            Receiver::Prefix(p, at) => {
                let saved = self.at_span(at);
                let v = self.gen_prefix(p, at)?;
                self.m.pos = saved;
                v
            }
        };
        let owner = self.method_owner(sym);
        let owner_ty = self.class_ref(owner);
        self.check_conforms(receiver, owner_ty, span)?;
        let statically = statically || matches!(self.pool.ty(owner_ty), Type::StructValue(_));
        if is_extern(self.l.u, sym) {
            return self.call_extern(sym, args, span);
        }
        let args = self.gen_args(sym, args, span)?;
        Ok(self.call_method(sym, statically, receiver, args))
    }

    /// A call of static method `sym`, named as a member of `owner` if given.
    pub fn gen_apply_static(
        &mut self,
        sym: SymbolId,
        owner: Option<SymbolId>,
        args: &[TNodeId],
        span: Span,
    ) -> Result<ValId> {
        let sig = self.method_sig(sym, true);
        let args = self.gen_args(sym, args, span)?;
        let name = self.static_member_name(sym, owner);
        let ptr = self.ptr(name);
        Ok(self.emit(Op::Call { ty: sig, ptr, args }))
    }

    /// A call of instance method `sym` on `receiver`, dispatched unless `statically`.
    pub fn call_method(
        &mut self,
        sym: SymbolId,
        statically: bool,
        receiver: ValId,
        args: Vec<ValId>,
    ) -> ValId {
        let name = self.method_name(sym);
        let ty = self.method_sig(sym, false);
        let ptr = if statically {
            self.ptr(name)
        } else {
            let nir::Global::Member(_, sig) = *self.pool.global(name) else { unreachable!("a member") };
            self.emit(Op::Method { obj: receiver, sig })
        };
        let args = std::iter::once(receiver).chain(args).collect();
        self.emit(Op::Call { ty, ptr, args })
    }

    fn ptr(&mut self, name: nir::GlobalId) -> ValId {
        let ptr = self.ty(Type::Ptr);
        self.val(Val::Global(name, ptr))
    }

    /// The value of the prefix `p` of a reference: an object, or `this`.
    fn gen_prefix(&mut self, p: tasty::TypeId, span: Span) -> Result<ValId> {
        match *self.l.u.types().get(p) {
            tasty::Type::This(c) => match self.l.u.class_symbol(c) {
                Some(c) => self.this_of(c, span),
                None => unsupported(span, "`this` of a type that is not a class"),
            },
            tasty::Type::Sym(_, s) if self.l.u.is_module(s) && crate::symbols::is_static(self.l.u, s) => {
                Ok(self.gen_module(s))
            }
            _ => unsupported(span, "a prefix that is not a static object or `this`"),
        }
    }
}
