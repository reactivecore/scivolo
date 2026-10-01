//! Calls: of methods, constructors and static methods (nscplugin's `genApply`).

use super::Gen;
use crate::symbols::{is_constructor, is_java_static, is_module, is_module_class};
use crate::{Result, unsupported};
use nir::{Op, Type, Val, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{List, TNodeId, Tree};

/// Whether binary class name `name` is a primitive value class.
pub fn is_primitive(name: &str) -> bool {
    let prims = ["Boolean", "Char", "Byte", "Short", "Int", "Long", "Float", "Double", "Unit"];
    name.strip_prefix("scala.").is_some_and(|n| prims.contains(&n))
}

/// The receiver of a call: a tree, or the prefix of an identifier's type.
#[derive(Clone, Copy)]
pub enum Receiver {
    Tree(TNodeId),
    Prefix(tasty::TypeId),
}

impl Gen<'_, '_> {
    pub fn gen_apply(&mut self, t: TNodeId, fun: TNodeId, args: List) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let saved = self.at_span(span);
        let args = self.l.tpd.items(args).to_vec();
        let Some(sym) = self.l.tpd.sym(fun) else {
            return unsupported(span, "a call without a method symbol");
        };
        let v = match *self.l.tpd.tree(fun) {
            Tree::TypeApply { .. } => return unsupported(span, "calls with type arguments"),
            Tree::Select { qualifier, .. } => match *self.l.tpd.tree(qualifier) {
                Tree::Super { .. } => {
                    let this = self.m.this.expect("`this` of a method");
                    let args = self.gen_args(&args)?;
                    self.call_method(sym, true, this, args)
                }
                Tree::New { tpt } if is_constructor(self.l.u, sym) => self.gen_apply_new(tpt, sym, &args)?,
                _ => self.gen_apply_call(sym, Receiver::Tree(qualifier), &args, span)?,
            },
            Tree::Ident { .. } => {
                let prefix = match *self.l.u.types().get(self.l.tpd.tpe(fun)) {
                    tasty::Type::Sym(Some(p), _) | tasty::Type::TermRef(p, _) => p,
                    _ => return unsupported(span, "a call of a local method"),
                };
                self.gen_apply_call(sym, Receiver::Prefix(prefix), &args, span)?
            }
            _ => return unsupported(span, "a call of a function value"),
        };
        self.m.pos = saved;
        Ok(v)
    }

    fn gen_apply_call(
        &mut self,
        sym: SymbolId,
        recv: Receiver,
        args: &[TNodeId],
        span: Span,
    ) -> Result<ValId> {
        if self.is_primitive_op(sym) {
            return unsupported(span, "primitive operations");
        }
        let statically = is_constructor(self.l.u, sym);
        self.gen_apply_method(sym, statically, recv, args)
    }

    fn gen_apply_new(&mut self, tpt: TNodeId, ctor: SymbolId, args: &[TNodeId]) -> Result<ValId> {
        let Some(class) = self.l.u.class_symbol(self.l.tpd.tpe(tpt)) else {
            return unsupported(self.l.tpd.span(tpt), "`new` of a type that is not a class");
        };
        let name = self.type_name(class);
        let alloc = self.emit(Op::Classalloc { name, zone: None });
        let args = self.gen_args(args)?;
        self.call_method(ctor, true, alloc, args);
        Ok(alloc)
    }

    pub fn gen_apply_method(
        &mut self,
        sym: SymbolId,
        statically: bool,
        recv: Receiver,
        args: &[TNodeId],
    ) -> Result<ValId> {
        if is_java_static(self.l.u, sym) {
            let owner = match recv {
                Receiver::Tree(t) => self.l.tpd.sym(t),
                Receiver::Prefix(p) => self.l.u.class_symbol(p),
            };
            let sig = self.method_sig(sym, true);
            let args = self.gen_args(args)?;
            let name = self.static_member_name(sym, owner);
            let ptr = self.ptr(name);
            return Ok(self.emit(Op::Call { ty: sig, ptr, args }));
        }
        let receiver = match recv {
            Receiver::Tree(t) => self.gen_expr(t)?,
            Receiver::Prefix(p) => self.gen_prefix(p)?,
        };
        let args = self.gen_args(args)?;
        Ok(self.call_method(sym, statically, receiver, args))
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

    /// Whether `sym` is an operation nscplugin generates inline: of `Any`, of a primitive class,
    /// reference equality, `synchronized`, string concatenation.
    fn is_primitive_op(&mut self, sym: SymbolId) -> bool {
        let Some(owner) = self.l.u.symbol(sym).owner else { return false };
        let name = self.l.u.symbol(sym).name.as_str();
        let owner_name = self.l.u.binary_name(owner);
        let object_ops = ["eq", "ne", "synchronized", "==", "!=", "##"];
        owner == self.l.defn.any
            || is_primitive(&owner_name)
            || (owner == self.l.defn.object && object_ops.contains(&name))
            || (owner_name == "java.lang.String" && name == "+")
    }

    fn gen_args(&mut self, args: &[TNodeId]) -> Result<Vec<ValId>> {
        args.iter().map(|&a| self.gen_expr(a)).collect()
    }

    fn ptr(&mut self, name: nir::GlobalId) -> ValId {
        let ptr = self.ty(Type::Ptr);
        self.val(Val::Global(name, ptr))
    }

    /// The value of the prefix `p` of a reference: an object, or `this`.
    fn gen_prefix(&mut self, p: tasty::TypeId) -> Result<ValId> {
        let class = self.class.expect("a class");
        match *self.l.u.types().get(p) {
            tasty::Type::This(c) => match self.l.u.class_symbol(c) {
                Some(c) if c != class && is_module_class(self.l.u, c) => Ok(self.gen_module(c)),
                Some(c) if c == class && self.m.this.is_some() => Ok(self.m.this.expect("`this`")),
                _ => unsupported(Span::NONE, "`this` of an enclosing class"),
            },
            tasty::Type::Sym(_, s) if is_module(self.l.u, s) => Ok(self.gen_module(s)),
            _ => unsupported(Span::NONE, "a prefix that is not an object or `this`"),
        }
    }
}
