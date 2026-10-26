// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirPrimitives.scala
//! The methods of `scala.scalanative.runtime.Intrinsics` generated inline, and raw loads,
//! stores, `elemRawPtr` and equality of raw pointers (nscplugin's `genApplyPrimitive`,
//! `genRawPtrOp`).

use super::Gen;
use crate::{Result, unsupported};
use nir::{Comp, Conv, MemoryOrder, Op, Type, TypeId, ValId};
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

pub const INTRINSICS: &str = "scala.scalanative.runtime.Intrinsics$";
const INTERNAL: &str = "scala.scalanative.runtime.Intrinsics$.internal$";

/// The type a raw load or store of `Intrinsics` method `name` accesses.
fn raw_type(name: &str) -> Option<&str> {
    let ty = name.strip_prefix("load").or_else(|| name.strip_prefix("store"))?;
    ["Boolean", "Char", "Byte", "Short", "Int", "Long", "Float", "Double", "RawPtr", "RawSize", "Object"]
        .contains(&ty)
        .then_some(ty)
}

impl Gen<'_, '_> {
    /// The operation call `app` of `fun`, method `sym`, with `args` stands for if an `Intrinsics`
    /// method or an equality of raw pointers; `None` for another call.
    pub fn gen_intrinsic(
        &mut self,
        app: TNodeId,
        sym: SymbolId,
        fun: TNodeId,
        args: &[TNodeId],
    ) -> Result<Option<ValId>> {
        let Some(owner) = self.l.u.symbol(sym).owner else { return Ok(None) };
        if [self.l.defs.any, self.l.defs.object].contains(&owner) {
            return self.gen_raw_ptr_equality(sym, fun, args);
        }
        let name = self.l.u.symbol(sym).name.clone();
        let span = self.l.tpd.span(app);
        let owner = self.l.u.full_name(owner);
        if owner == INTERNAL
            && let ("sizeOf" | "alignmentOf", &[cls]) = (name.as_str(), args)
            && let Tree::ClassLiteral { tpe } = *self.l.tpd.tree(cls)
        {
            let ty = self.gen_type(tpe);
            return Ok(Some(self.emit(if name == "sizeOf" { Op::SizeOf(ty) } else { Op::AlignmentOf(ty) })));
        }
        if super::callbacks::is_from_function(&owner, &name) {
            return self.gen_func_ptr_from_function(app, args, span).map(Some);
        }
        if super::funcptr::is_func_ptr_apply(&owner, &name) {
            return self.gen_func_ptr_apply(fun, args, span).map(Some);
        }
        if owner == super::cquote::CQUOTE && name == "c" {
            let (Tree::Select { qualifier, .. }, []) = (self.l.tpd.tree(fun).clone(), args) else {
                return unsupported(span, "a C string literal that is not a call");
            };
            return self.gen_cquote(qualifier, span).map(Some);
        }
        if owner != INTRINSICS {
            return Ok(None);
        }
        if ["sizeOf", "alignmentOf", "stackalloc"].contains(&name.as_str()) {
            let Some((targs, targs_span)) = self.l.non_erased.get(&fun).cloned() else {
                return unsupported(span, format!("`{name}` without its type argument"));
            };
            let saved = (name != "stackalloc").then(|| self.at_span(targs_span));
            let v = self.gen_layout_op(&name, &targs, args, span);
            if let Some(saved) = saved {
                self.m.pos = saved;
            }
            return v;
        }
        let v = match (name.as_str(), args) {
            (n, &[ptrp])
                if n.starts_with("load")
                    && let Some(ty) = raw_type(n) =>
            {
                let ty = self.raw_type(ty);
                let ptr = self.gen_expr(ptrp)?;
                let order = self.memory_order(ptrp, MemoryOrder::Acquire);
                self.emit(Op::Load { ty, ptr, order })
            }
            (n, &[ptrp, valuep])
                if n.starts_with("store")
                    && let Some(ty) = raw_type(n) =>
            {
                let ty = self.raw_type(ty);
                let ptr = self.gen_expr(ptrp)?;
                let value = self.gen_expr(valuep)?;
                let order = self.memory_order(ptrp, MemoryOrder::Release);
                self.emit(Op::Store { ty, ptr, value, order })
            }
            ("elemRawPtr", &[ptrp, offsetp]) => {
                let ptr = self.gen_expr(ptrp)?;
                let offset = self.gen_expr(offsetp)?;
                let ty = self.ty(Type::Byte);
                self.emit(Op::Elem { ty, ptr, indexes: vec![offset] })
            }
            ("classFieldRawPtr", &[target, field]) => self.gen_class_field_raw_ptr(target, field, span)?,
            _ => return self.gen_conversion(app, (sym, &name), args, span),
        };
        Ok(Some(v))
    }

    fn raw_type(&mut self, name: &str) -> TypeId {
        let t = match name {
            "Boolean" => Type::Bool,
            "Char" => Type::Char,
            "Byte" => Type::Byte,
            "Short" => Type::Short,
            "Int" => Type::Int,
            "Long" => Type::Long,
            "Float" => Type::Float,
            "Double" => Type::Double,
            "RawPtr" => Type::Ptr,
            "RawSize" => Type::Size,
            _ => return self.class_type("java.lang.Object", false),
        };
        self.ty(t)
    }

    /// `volatile` if the pointer `ptrp` is a volatile value, else unordered.
    fn memory_order(&self, ptrp: TNodeId, volatile: MemoryOrder) -> Option<MemoryOrder> {
        let is_volatile = self.l.tpd.sym(ptrp).is_some_and(|s| self.l.u.is_annotated(s, "scala.volatile"));
        Some(if is_volatile { volatile } else { MemoryOrder::Unordered })
    }

    /// `left op right` for `op` one of `==`, `!=`, `eq` and `ne` of `Any` on two raw pointers, a raw
    /// pointer and a reference, or two raw sizes (nscplugin's `genBinaryOp` on `ptr` and `size`).
    fn gen_raw_ptr_equality(
        &mut self,
        sym: SymbolId,
        fun: TNodeId,
        args: &[TNodeId],
    ) -> Result<Option<ValId>> {
        let (Tree::Select { qualifier: left, .. }, &[right]) = (self.l.tpd.tree(fun).clone(), args) else {
            return Ok(None);
        };
        let op = self.l.u.symbol(sym).name.clone();
        let comp = match op.as_str() {
            "==" | "eq" | "!=" => Comp::Ieq,
            "ne" => Comp::Ine,
            _ => return Ok(None),
        };
        let (lty, rty) = (self.gen_type(self.l.tpd.tpe(left)), self.gen_type(self.l.tpd.tpe(right)));
        let reference = |t: &Type| matches!(t, Type::Ref { .. } | Type::Array { .. } | Type::Null);
        let ty = match (self.pool.ty(lty), self.pool.ty(rty)) {
            (Type::Ptr, r) if *r == Type::Ptr || reference(r) => lty,
            (l, Type::Ptr) if reference(l) => rty,
            (Type::Size, Type::Size) => lty,
            _ => return Ok(None),
        };
        let saved = self.at_span(self.l.tpd.span(left));
        let l = self.gen_expr(left)?;
        let l = if lty == ty { l } else { self.emit(Op::Conv { conv: Conv::Bitcast, ty, value: l }) };
        self.at_span(self.l.tpd.span(right));
        let r = self.gen_expr(right)?;
        let r = if rty == ty { r } else { self.emit(Op::Conv { conv: Conv::Bitcast, ty, value: r }) };
        self.at_span(self.l.tpd.span(left));
        let v = self.emit(Op::Comp { comp, ty, l, r });
        self.m.pos = saved;
        if op == "!=" {
            let (bool, tru) = (self.ty(Type::Bool), self.val(nir::Val::True));
            return Ok(Some(self.emit(Op::Bin { bin: nir::Bin::Xor, ty: bool, l: tru, r: v })));
        }
        Ok(Some(v))
    }
}
