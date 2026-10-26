// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirPrimitives.scala
//! The methods of `scala.scalanative.runtime.Intrinsics` generated inline: raw loads and stores,
//! `elemRawPtr`, casts, unsigned operations, `classFieldRawPtr` (nscplugin's `genRawPtrOp`,
//! `genRawPtrCastOp`, `genRawSizeCastOp`, `genUnsignedOp`, `genClassFieldRawPtr`).

use super::Gen;
use crate::{Result, unsupported};
use nir::{Bin, Comp, Conv, MemoryOrder, Op, Type, TypeId, ValId};
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

pub const INTRINSICS: &str = "scala.scalanative.runtime.Intrinsics$";

/// The type a raw load or store of `Intrinsics` method `name` accesses.
fn raw_type(name: &str) -> Option<&str> {
    let ty = name.strip_prefix("load").or_else(|| name.strip_prefix("store"))?;
    ["Boolean", "Char", "Byte", "Short", "Int", "Long", "Float", "Double", "RawPtr", "RawSize", "Object"]
        .contains(&ty)
        .then_some(ty)
}

/// The conversion and result type of a raw size cast.
fn size_cast(name: &str) -> Option<(Conv, Type)> {
    Some(match name {
        "castRawSizeToInt" => (Conv::SSizeCast, Type::Int),
        "castRawSizeToLong" => (Conv::SSizeCast, Type::Long),
        "castRawSizeToLongUnsigned" => (Conv::ZSizeCast, Type::Long),
        "castIntToRawSize" | "castLongToRawSize" => (Conv::SSizeCast, Type::Size),
        "castIntToRawSizeUnsigned" => (Conv::ZSizeCast, Type::Size),
        _ => return None,
    })
}

/// The conversion of an unsigned operation to a wider integer or a float.
fn unsigned_conv(name: &str) -> Option<Conv> {
    match name {
        "byteToUInt" | "byteToULong" | "shortToUInt" | "shortToULong" | "intToULong" => Some(Conv::Zext),
        "uintToFloat" | "ulongToFloat" | "uintToDouble" | "ulongToDouble" => Some(Conv::Uitofp),
        _ => None,
    }
}

const RAW_PTR_CASTS: [&str; 10] = [
    "castRawPtrToObject",
    "castObjectToRawPtr",
    "castIntToFloat",
    "castFloatToInt",
    "castLongToDouble",
    "castDoubleToLong",
    "castRawPtrToInt",
    "castRawPtrToLong",
    "castIntToRawPtr",
    "castLongToRawPtr",
];

/// The conversion of a cast from `from` to `to`, none if they are equal (nscplugin's `castConv`).
fn cast_conv(from: &Type, to: &Type) -> Option<std::result::Result<Conv, ()>> {
    let is_ref = |t: &Type| matches!(t, Type::Ref { .. } | Type::Array { .. } | Type::Null | Type::Nothing);
    let is_int = |t: &Type| {
        matches!(t, Type::Bool | Type::Char | Type::Byte | Type::Short | Type::Int | Type::Long | Type::Size)
    };
    let width = |t: &Type| match t {
        Type::Int | Type::Float => Some(32),
        Type::Long | Type::Double => Some(64),
        _ => None,
    };
    let float = |t: &Type| matches!(t, Type::Float | Type::Double);
    Some(Ok(match (from, to) {
        (f, Type::Ptr) if is_int(f) => Conv::Inttoptr,
        (Type::Ptr, t) if is_int(t) => Conv::Ptrtoint,
        (f, Type::Ptr) if is_ref(f) => Conv::Bitcast,
        (Type::Ptr, t) if is_ref(t) => Conv::Bitcast,
        (f, t) if is_ref(f) && is_ref(t) => Conv::Bitcast,
        (f, t) if is_ref(f) && is_int(t) => Conv::Ptrtoint,
        (f, t) if is_int(f) && is_ref(t) => Conv::Inttoptr,
        (f, t) if is_int(f) && float(t) && width(f).is_some() && width(f) == width(t) => Conv::Bitcast,
        (f, t) if float(f) && is_int(t) && width(t).is_some() && width(f) == width(t) => Conv::Bitcast,
        _ if from == to => return None,
        (Type::Float, Type::Double) => Conv::Fpext,
        (Type::Double, Type::Float) => Conv::Fptrunc,
        _ => return Some(Err(())),
    }))
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
        if self.l.u.full_name(owner) != INTRINSICS {
            return Ok(None);
        }
        let name = self.l.u.symbol(sym).name.clone();
        let span = self.l.tpd.span(app);
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
            (n, &[argp]) if RAW_PTR_CASTS.contains(&n) => {
                let from = self.gen_type(self.l.tpd.tpe(argp));
                let to = self.gen_type(self.l.tpd.tpe(app));
                let value = self.gen_expr(argp)?;
                let value_ty = self.val_ty(value);
                let (f, t, vt) =
                    (self.pool.ty(from).clone(), self.pool.ty(to).clone(), self.pool.ty(value_ty).clone());
                match cast_conv(&f, &t).or_else(|| cast_conv(&vt, &t)) {
                    None => value,
                    Some(Ok(conv)) => self.emit(Op::Conv { conv, ty: to, value }),
                    Some(Err(())) => return unsupported(span, format!("a cast from {f:?} to {t:?}")),
                }
            }
            (n, &[argp]) if let Some((conv, ty)) = size_cast(n) => {
                let value = self.gen_expr(argp)?;
                let ty = self.ty(ty);
                self.emit(Op::Conv { conv, ty, value })
            }
            ("unsignedOf", &[argp]) => {
                let ty = self.result_type(sym);
                let obj = self.gen_expr(argp)?;
                self.emit(Op::Box { ty, obj })
            }
            (n, &[argp]) if let Some(conv) = unsigned_conv(n) => {
                let ty = self.gen_type(self.l.tpd.tpe(app));
                let value = self.gen_expr(argp)?;
                self.emit(Op::Conv { conv, ty, value })
            }
            ("divUInt" | "divULong" | "remUInt" | "remULong", &[leftp, rightp]) => {
                let bin = if name.starts_with("div") { Bin::Udiv } else { Bin::Urem };
                let ty = self.gen_type(self.l.tpd.tpe(leftp));
                let l = self.gen_expr(leftp)?;
                let r = self.gen_expr(rightp)?;
                self.emit(Op::Bin { bin, ty, l, r })
            }
            ("classFieldRawPtr", &[target, field]) => self.gen_class_field_raw_ptr(target, field, span)?,
            _ => return Ok(None),
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

    /// `left op right` for `op` one of `==`, `!=`, `eq` and `ne` of `Any` on two raw pointers.
    fn gen_raw_ptr_equality(
        &mut self,
        sym: SymbolId,
        fun: TNodeId,
        args: &[TNodeId],
    ) -> Result<Option<ValId>> {
        let (Tree::Select { qualifier: left, .. }, &[right]) = (self.l.tpd.tree(fun).clone(), args) else {
            return Ok(None);
        };
        let comp = match self.l.u.symbol(sym).name.as_str() {
            "==" | "eq" => Comp::Ieq,
            "!=" | "ne" => Comp::Ine,
            _ => return Ok(None),
        };
        let ptr = self.ty(Type::Ptr);
        let lty = self.gen_type(self.l.tpd.tpe(left));
        if lty != ptr || self.gen_type(self.l.tpd.tpe(right)) != ptr {
            return Ok(None);
        }
        let saved = self.at_span(self.l.tpd.span(left));
        let l = self.gen_expr(left)?;
        self.at_span(self.l.tpd.span(right));
        let r = self.gen_expr(right)?;
        self.at_span(self.l.tpd.span(left));
        let v = self.emit(Op::Comp { comp, ty: ptr, l, r });
        self.m.pos = saved;
        Ok(Some(v))
    }
}
