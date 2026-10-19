// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! Numeric types: their widths, the type both operands of an operation take (nscplugin's
//! `binaryOperationType`), conversions between them (`genCoercion`).

use super::Gen;
use nir::{Conv, Op, Type, TypeId, ValId};

/// The width and signedness of an integer type.
pub fn int(t: &Type) -> Option<(u32, bool)> {
    Some(match t {
        Type::Bool => (1, false),
        Type::Char => (16, false),
        Type::Byte => (8, true),
        Type::Short => (16, true),
        Type::Int => (32, true),
        Type::Long => (64, true),
        _ => return None,
    })
}

/// The width of a floating-point type.
pub fn float(t: &Type) -> Option<u32> {
    match t {
        Type::Float => Some(32),
        Type::Double => Some(64),
        _ => None,
    }
}

impl Gen<'_, '_> {
    /// The type both operands of a binary operation are converted to.
    pub fn operation_type(&mut self, lty: TypeId, rty: TypeId) -> TypeId {
        let (l, r) = (self.pool.ty(lty).clone(), self.pool.ty(rty).clone());
        let wider = |lw: u32, rw: u32| if lw >= rw { lty } else { rty };
        match (&l, &r) {
            (Type::Long, Type::Float) => self.ty(Type::Double),
            (Type::Bool, Type::Bool) => lty,
            _ => match (int(&l), int(&r), float(&l), float(&r)) {
                (Some((lw, _)), Some((rw, _)), _, _) if lw < 32 && rw < 32 => self.ty(Type::Int),
                (Some((lw, _)), Some((rw, _)), _, _) => wider(lw, rw),
                (Some(_), _, _, Some(_)) => rty,
                (_, Some(_), Some(_), _) => lty,
                (_, _, Some(lw), Some(rw)) => wider(lw, rw),
                _ if l == Type::Nothing => rty,
                _ => lty,
            },
        }
    }

    /// `value` of type `from` converted to `to`.
    pub fn coerce(&mut self, value: ValId, from: TypeId, to: TypeId) -> ValId {
        let (f, t) = (self.pool.ty(from).clone(), self.pool.ty(to).clone());
        if f == t || f == Type::Nothing || t == Type::Nothing {
            return value;
        }
        let conv = match (int(&f), int(&t), float(&f), float(&t)) {
            (Some((fw, signed)), Some((tw, _)), _, _) if fw < tw => {
                if signed {
                    Conv::Sext
                } else {
                    Conv::Zext
                }
            }
            (Some((fw, _)), Some((tw, _)), _, _) if fw > tw => Conv::Trunc,
            (Some(_), Some(_), _, _) => Conv::Bitcast,
            (Some((_, true)), _, _, Some(_)) => Conv::Sitofp,
            (Some(_), _, _, Some(_)) => Conv::Uitofp,
            (_, Some((tw, signed)), Some(_), _) => {
                if tw < 32 {
                    let int_ty = self.ty(Type::Int);
                    let as_int = self.coerce(value, from, int_ty);
                    return self.coerce(as_int, int_ty, to);
                }
                if signed { Conv::Fptosi } else { Conv::Fptoui }
            }
            (_, _, Some(64), Some(32)) => Conv::Fptrunc,
            (_, _, Some(32), Some(64)) => Conv::Fpext,
            _ => Conv::Bitcast,
        };
        self.emit(Op::Conv { conv, ty: to, value })
    }
}
