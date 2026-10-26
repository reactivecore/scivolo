// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! The casts and unsigned operations of `Intrinsics` (nscplugin's `genRawPtrCastOp`,
//! `genRawSizeCastOp`, `genUnsignedOp`, `castConv`).

use super::Gen;
use crate::{Result, unsupported};
use nir::{Bin, Conv, Op, Type, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::TNodeId;

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

/// The conversion of a cast from `from` to `to`, none if they are equal, an error if there is
/// none (nscplugin's `castConv`).
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
    /// Call `app` of cast or unsigned operation `sym` of `Intrinsics`, `name`, with `args`;
    /// `None` for another method.
    pub fn gen_conversion(
        &mut self,
        app: TNodeId,
        (sym, name): (SymbolId, &str),
        args: &[TNodeId],
        span: Span,
    ) -> Result<Option<ValId>> {
        let v = match (name, args) {
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
            _ => return Ok(None),
        };
        Ok(Some(v))
    }
}
