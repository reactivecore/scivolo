// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! Calls of extern methods (nscplugin's `genApplyMethod` of an extern symbol,
//! `genExternMethodArgs`, `toExtern`, `fromExtern`).

use super::Gen;
use super::externs::unboxed;
use crate::symbols::has;
use crate::{Result, unsupported};
use nir::{Op, Type, TypeId, Val, ValId};
use syntax::ast::Span;
use tasty::tags::FIELDaccessor;
use tasty::{Kind, SymbolId};
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    /// A call of extern method `sym` with `args`: unboxed arguments, the result boxed back.
    pub fn call_extern(&mut self, sym: SymbolId, args: &[TNodeId], span: Span) -> Result<ValId> {
        if has(self.l.u, sym, FIELDaccessor) || self.l.u.symbol(sym).kind != Kind::Def {
            return unsupported(span, "extern fields");
        }
        let ty = self.extern_sig(sym, span)?;
        let Type::Function(params, _) = self.pool.ty(ty).clone() else { unreachable!("a method type") };
        let orig = self.method_sig(sym, true);
        let Type::Function(orig_params, orig_ret) = self.pool.ty(orig).clone() else {
            unreachable!("a method type")
        };
        if params.len() != args.len() {
            return unsupported(span, "a call with several argument lists");
        }
        let mut values = Vec::new();
        for ((&a, &param), &orig_param) in args.iter().zip(&params).zip(&orig_params) {
            let v = self.gen_expr(a)?;
            let primitive =
                !matches!(*self.pool.ty(param), Type::Ptr | Type::Ref { .. } | Type::Array { .. });
            if *self.pool.val(v) == Val::Null && primitive {
                return unsupported(self.l.tpd.span(a), "null as a primitive extern argument");
            }
            self.check_conforms(v, orig_param, self.l.tpd.span(a))?;
            let saved = self.at_span(self.l.tpd.span(a));
            values.push(self.extern_arg(param, v));
            self.m.pos = saved;
        }
        let owner = self.l.u.symbol(sym).owner.expect("a method's owner");
        let name = self.extern_name(owner, sym, span)?;
        let ptr_ty = self.ty(Type::Ptr);
        let ptr = self.val(Val::Global(name, ptr_ty));
        let result = self.emit(Op::Call { ty, ptr, args: values });
        Ok(self.extern_result(orig_ret, result))
    }

    /// `v` as extern type `expected`: unboxed if `v` is its box.
    pub fn extern_arg(&mut self, expected: TypeId, v: ValId) -> ValId {
        if *self.pool.ty(expected) == Type::Unit {
            return self.val(Val::Unit);
        }
        let ty = self.val_ty(v);
        match self.pool.ty(ty).clone() {
            Type::Ref { name, .. } if self.boxes(name, expected) => {
                let ty = self.ty(Type::Ref { name, exact: false, nullable: true });
                self.emit(Op::Unbox { ty, obj: v })
            }
            _ => v,
        }
    }

    /// Extern result `v` as type `expected`: boxed if `expected` is the box of its type.
    pub fn extern_result(&mut self, expected: TypeId, v: ValId) -> ValId {
        let ty = self.val_ty(v);
        match self.pool.ty(expected).clone() {
            Type::Ref { name, .. } if self.boxes(name, ty) => {
                let ty = self.ty(Type::Ref { name, exact: false, nullable: true });
                self.emit(Op::Box { ty, obj: v })
            }
            _ => v,
        }
    }

    /// Whether class `name` boxes values of type `ty`.
    fn boxes(&self, name: nir::GlobalId, ty: TypeId) -> bool {
        let class = self.pool.top_name(name).to_owned();
        unboxed(&class).is_some_and(|t| *self.pool.ty(ty) == t)
    }
}
