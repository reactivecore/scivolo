// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! Calls of extern methods and accesses of extern fields (nscplugin's `genApplyMethod` of an
//! extern symbol, `genExternMethodArgs`, `genLoadExtern`, `genStoreExtern`, `toExtern`,
//! `fromExtern`).

use super::Gen;
use super::externs::unboxed;
use crate::symbols::{has, is_extern};
use crate::{Result, unsupported};
use nir::{MemoryOrder, Op, Type, TypeId, Val, ValId};
use syntax::ast::Span;
use tasty::tags::FIELDaccessor;
use tasty::{Kind, SymbolId};
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    /// Whether `sym` is an extern field or its accessor.
    pub fn is_extern_field(&mut self, sym: SymbolId) -> bool {
        is_extern(self.l.u, sym)
            && (has(self.l.u, sym, FIELDaccessor) || self.l.u.symbol(sym).kind != Kind::Def)
    }

    /// The C signature of extern method `sym` without variable arguments.
    pub fn extern_call_sig(&mut self, sym: SymbolId, span: Span) -> Result<TypeId> {
        let ty = self.extern_sig(sym, span)?;
        let Type::Function(params, _) = self.pool.ty(ty) else { unreachable!("a method type") };
        if params.iter().any(|&p| matches!(self.pool.ty(p), Type::Vararg)) {
            return unsupported(span, "extern calls with variable arguments");
        }
        Ok(ty)
    }

    /// A call of extern method `sym` with `args`: unboxed arguments, the result boxed back.
    pub fn call_extern(&mut self, sym: SymbolId, args: &[TNodeId], span: Span) -> Result<ValId> {
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
            if *self.pool.ty(param) == Type::Vararg {
                values.extend(self.extern_varargs(a, span)?);
                continue;
            }
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
        let ptr = self.extern_global(sym, span)?;
        let result = self.emit(Op::Call { ty, ptr, args: values });
        Ok(self.extern_result(orig_ret, result))
    }

    /// A read of extern field `sym`, through its getter without `args`.
    pub fn load_extern_field(&mut self, sym: SymbolId, args: &[TNodeId], span: Span) -> Result<ValId> {
        if !args.is_empty() {
            return unsupported(span, "extern field setters");
        }
        let (ty, orig) = self.extern_field_types(sym, span)?;
        let ptr = self.extern_global(sym, span)?;
        let order = self.is_volatile(sym).then_some(MemoryOrder::Acquire);
        let result = self.emit(Op::Load { ty, ptr, order });
        Ok(self.extern_result(orig, result))
    }

    /// `sym = rhs` for extern field `sym`.
    pub fn store_extern_field(&mut self, sym: SymbolId, rhs: TNodeId, span: Span) -> Result<ValId> {
        let (ty, orig) = self.extern_field_types(sym, span)?;
        let saved = self.at_span(span);
        let value = self.gen_expr(rhs)?;
        self.check_conforms(value, orig, span)?;
        let value = self.extern_arg(ty, value);
        let ptr = self.extern_global(sym, span)?;
        let order = self.is_volatile(sym).then_some(MemoryOrder::Release);
        let v = self.emit(Op::Store { ty, ptr, value, order });
        self.m.pos = saved;
        Ok(v)
    }

    /// The C and the Scala type of extern field `sym`.
    fn extern_field_types(&mut self, sym: SymbolId, span: Span) -> Result<(TypeId, TypeId)> {
        let ty = self.extern_sig(sym, span)?;
        let Type::Function(params, ret) = self.pool.ty(ty).clone() else { unreachable!("a method type") };
        if !params.is_empty() {
            return unsupported(span, "extern field setters");
        }
        let orig = self.method_sig(sym, true);
        let Type::Function(_, orig_ret) = self.pool.ty(orig).clone() else { unreachable!("a method type") };
        Ok((ret, orig_ret))
    }

    /// Whether `sym` or the field made for it is `@volatile`.
    fn is_volatile(&mut self, sym: SymbolId) -> bool {
        let field = [Some(sym), self.l.fields.get(&sym).copied()];
        field.into_iter().flatten().any(|s| self.l.u.is_annotated(s, "scala.volatile"))
    }

    /// The address of extern method or field `sym`.
    fn extern_global(&mut self, sym: SymbolId, span: Span) -> Result<ValId> {
        let owner = self.l.u.symbol(sym).owner.expect("a member's owner");
        let name = self.extern_name(owner, sym, span)?;
        let ptr_ty = self.ty(Type::Ptr);
        Ok(self.val(Val::Global(name, ptr_ty)))
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
