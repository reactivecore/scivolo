// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! Calls of C function pointers: `CFuncPtrN.apply`, arguments unboxed, the result boxed
//! (nscplugin's `genCFuncPtrApply`).

use super::Gen;
use super::externs::unboxed;
use crate::{Result, unsupported};
use nir::sig::{Scope, Sig};
use nir::{Op, Type, TypeId, ValId};
use syntax::ast::Span;
use typer::tpd::{TNodeId, Tree};

/// Whether class `owner`, a full name, and method `name` are `CFuncPtrN.apply`.
pub fn is_func_ptr_apply(owner: &str, name: &str) -> bool {
    let arity = owner.strip_prefix("scala.scalanative.unsafe.CFuncPtr").and_then(|n| n.parse::<u8>().ok());
    name == "apply" && arity.is_some_and(|n| n <= 22)
}

impl Gen<'_, '_> {
    /// `fun(args)` for `fun` a `CFuncPtrN.apply`: a call of the function pointer the receiver holds.
    pub fn gen_func_ptr_apply(&mut self, fun: TNodeId, args: &[TNodeId], span: Span) -> Result<ValId> {
        let Some((types, _)) = self.l.non_erased.get(&fun).cloned() else {
            return unsupported(span, "a call of a C function pointer without its types");
        };
        let (Tree::Select { qualifier, .. }, Some((&result, params))) =
            (self.l.tpd.tree(fun).clone(), types.split_last())
        else {
            return unsupported(span, "a call of a C function pointer without a receiver");
        };
        if params.len() != args.len() {
            return unsupported(span, "a call of a C function pointer with other arguments");
        }
        let receiver = self.gen_expr(qualifier)?;
        let ret = self.gen_type(result);
        let unboxed_ret = self.unboxed_type(ret).unwrap_or(ret);
        let mut values = Vec::new();
        for (&arg, &param) in args.iter().zip(params) {
            let saved = self.at_span(self.l.tpd.span(arg));
            let ty = self.gen_type(param);
            let obj = self.gen_expr(arg)?;
            let v = match self.unboxed_type(ty) {
                Some(_) => self.emit(Op::Unbox { ty, obj }),
                None => self.unbox_value(ty, obj),
            };
            values.push(v);
            self.m.pos = saved;
        }
        let arg_types = values.iter().map(|&v| self.val_ty(v)).collect();
        let sig = self.ty(Type::Function(arg_types, unboxed_ret));
        let func_ptr = self.pool.top("scala.scalanative.unsafe.CFuncPtr");
        let name = self.pool.member(func_ptr, &Sig::Field("rawptr".into(), Scope::Private(func_ptr)));
        let ptr_ty = self.ty(Type::Ptr);
        let target = self.emit(Op::Fieldload { ty: ptr_ty, obj: receiver, name });
        let result = self.emit(Op::Call { ty: sig, ptr: target, args: values });
        if ret != unboxed_ret {
            return Ok(self.emit(Op::Box { ty: ret, obj: result }));
        }
        Ok(match self.boxed_class(ret) {
            Some(boxed) => self.emit(Op::Box { ty: boxed, obj: result }),
            None => result,
        })
    }

    /// The C type of box class type `t`, if it is one (NIR's `Type.unbox`).
    fn unboxed_type(&mut self, t: TypeId) -> Option<TypeId> {
        let Type::Ref { name, .. } = *self.pool.ty(t) else { return None };
        let u = unboxed(self.pool.top_name(name))?;
        Some(self.ty(u))
    }

    /// `v` unboxed to primitive type `ty` unless it is one (nscplugin's `unboxValue`).
    fn unbox_value(&mut self, ty: TypeId, v: ValId) -> ValId {
        let Some(boxed) = self.boxed_class(ty) else { return v };
        let vt = self.val_ty(v);
        match self.pool.ty(vt) {
            t if super::numeric::int(t).is_some() || super::numeric::float(t).is_some() => v,
            _ => self.emit(Op::Unbox { ty: boxed, obj: v }),
        }
    }
}
