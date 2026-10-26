// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! C function pointers to Scala functions: `CFuncPtrN.fromScalaFunction` of a closure, whose
//! anonymous class gets an extern forwarder to the closure's method (nscplugin's
//! `genCFuncFromScalaFunction`, `genFuncExternForwarder`).

use super::Gen;
use crate::symbols::is_java_static;
use crate::{Result, unsupported};
use nir::sig::Sig;
use nir::{Attr, Defn, DefnKind, GlobalId, Op, Type, Val, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

const FORWARDER: &str = "$extern$forwarder";

/// Whether object `owner`, a full name, and method `name` are `CFuncPtrN.fromScalaFunction`.
pub fn is_from_function(owner: &str, name: &str) -> bool {
    let arity = owner.strip_prefix("scala.scalanative.unsafe.CFuncPtr").and_then(|n| n.strip_suffix('$'));
    name == "fromScalaFunction" && arity.and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| n <= 22)
}

impl Gen<'_, '_> {
    /// `fromScalaFunction(fn)`, call `app`: a function pointer to the extern forwarder of closure
    /// `fn`'s anonymous class.
    pub fn gen_func_ptr_from_function(
        &mut self,
        app: TNodeId,
        args: &[TNodeId],
        span: Span,
    ) -> Result<ValId> {
        let &[mut f] = args else { return unsupported(span, "`fromScalaFunction` of other arguments") };
        while let Tree::Typed { expr, .. } | Tree::Block { expr, .. } = *self.l.tpd.tree(f) {
            f = expr;
        }
        let Tree::Closure { env, meth, tpt } = *self.l.tpd.tree(f) else {
            return unsupported(span, "a C function pointer to a function that is not a lambda");
        };
        if !self.l.tpd.items(env).is_empty() {
            return unsupported(span, "a C function pointer to a lambda capturing values");
        }
        let target = self.l.tpd.sym(meth).expect("a closure's method");
        let owner = self.l.u.symbol(target).owner.expect("a method's owner");
        if !is_java_static(self.l.u, target) && !crate::symbols::is_static_module(self.l.u, owner) {
            return unsupported(span, "a C function pointer to a lambda using `this`");
        }
        let closure = self.gen_closure(f, env, meth, tpt)?;
        let closure_ty = self.val_ty(closure);
        let Type::Ref { name: anon, .. } = *self.pool.ty(closure_ty) else {
            unreachable!("a closure's class")
        };
        let forwarder = self.gen_extern_forwarder(anon, target, span)?;
        self.closures.push(forwarder);
        let Some(class) = self.l.u.class_symbol(self.l.tpd.tpe(app)) else {
            return unsupported(span, "`fromScalaFunction` of a type that is not a class");
        };
        let class = self.type_name(class);
        let class_ty = self.ty(Type::Ref { name: class, exact: false, nullable: true });
        let (ptr_ty, unit) = (self.ty(Type::Ptr), self.ty(Type::Unit));
        let ctor_ty = self.ty(Type::Function(vec![class_ty, ptr_ty], unit));
        let ctor = self.pool.member(class, &Sig::Ctor(vec![ptr_ty]));
        let sig = self.pool.sig(&Sig::Generated(FORWARDER.into()));
        let raw = self.emit(Op::Method { obj: closure, sig });
        let alloc = self.emit(Op::Classalloc { name: class, zone: None });
        let ptr = self.global_ptr(ctor);
        self.emit(Op::Call { ty: ctor_ty, ptr, args: vec![alloc, raw] });
        Ok(alloc)
    }

    /// The extern forwarder of anonymous class `anon` to static or object method `target`, its
    /// arguments boxed and result unboxed.
    fn gen_extern_forwarder(&mut self, anon: GlobalId, target: SymbolId, span: Span) -> Result<Defn> {
        if self.l.u.symbol(target).name.contains("$adapted$") {
            return unsupported(span, "a C function pointer to an adapted lambda");
        }
        let static_target = is_java_static(self.l.u, target);
        let ty = self.extern_sig(target, span)?;
        let Type::Function(params, ret) = self.pool.ty(ty).clone() else { unreachable!("a method type") };
        let orig_params = self.param_types(target);
        let saved = self.at_span(span);
        let body = self.closure_body(|g| {
            let params: Vec<_> = params.iter().map(|&t| g.param(t)).collect();
            let entry = g.fresh();
            g.label(entry, params.clone());
            let mut args = Vec::new();
            for (p, &orig) in params.iter().zip(&orig_params) {
                let v = g.val(Val::Local(p.id, p.ty));
                args.push(g.extern_result(orig, v));
            }
            let owner = g.l.u.symbol(target).owner.expect("a method's owner");
            let result = if static_target {
                let call_ty = g.method_sig(target, true);
                let ptr = g.static_member_name(target, Some(owner));
                let ptr = g.global_ptr(ptr);
                g.emit(Op::Call { ty: call_ty, ptr, args })
            } else {
                let module = g.gen_module(owner);
                g.call_method(target, true, module, args)
            };
            let result = g.extern_arg(ret, result);
            g.ret(result);
        });
        let pos = self.m.pos;
        self.m.pos = saved;
        let name = self.pool.member(anon, &Sig::Generated(FORWARDER.into()));
        let attrs = vec![Attr::Extern { blocking: false }];
        Ok(Defn { name, attrs, pos, kind: DefnKind::Define { ty, body } })
    }
}
