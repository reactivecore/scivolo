// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/PostInlineNativeInterop.scala
//! `sizeOf`, `alignmentOf` and `stackalloc` of a type argument (nscplugin's `genSizeOf`,
//! `genAlignmentOf`, `genStackalloc`, `getUnboxedSize`).

use super::Gen;
use super::externs::unboxed;
use crate::{Result, unsupported};
use nir::{Conv, Op, Type, Val, ValId};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use typer::tpd::{TNodeId, Tree};

/// The calls `stackalloc`'s size argument is taken through, by owner: conversions between
/// integers and sizes.
const SIZE_CONVERSIONS: [(&str, &[&str]); 4] = [
    (
        "scala.scalanative.runtime.Intrinsics$",
        &[
            "castIntToRawSize",
            "castIntToRawSizeUnsigned",
            "castLongToRawSize",
            "castRawSizeToInt",
            "castRawSizeToLong",
            "castRawSizeToLongUnsigned",
            "unsignedOf",
        ],
    ),
    ("scala.scalanative.unsafe.Size$", &["byteToSize", "shortToSize", "intToSize"]),
    ("scala.scalanative.unsigned.USize$", &["ubyteToUSize", "ushortToUSize", "uintToUSize"]),
    ("scala.scalanative.runtime.package$", &["fromRawSize", "fromRawUSize", "toRawSize"]),
];

impl Gen<'_, '_> {
    /// `sizeOf[T]`, `alignmentOf[T]` or `stackalloc[T](size?)` of `Intrinsics`, `name`, with type
    /// arguments `targs` and arguments `args`.
    pub fn gen_layout_op(
        &mut self,
        name: &str,
        targs: &[tasty::TypeId],
        args: &[TNodeId],
        span: Span,
    ) -> Result<Option<ValId>> {
        let &[tpe] = targs else { return Ok(None) };
        let class = self.layout_class(tpe);
        let v = match (name, args) {
            ("sizeOf" | "alignmentOf", []) => {
                if class.is_some_and(|c| self.l.u.is_trait(c)) {
                    return unsupported(span, format!("`{name}` of a trait"));
                }
                let ty = self.exact_type(tpe, span)?;
                self.emit(if name == "sizeOf" { Op::SizeOf(ty) } else { Op::AlignmentOf(ty) })
            }
            ("stackalloc", []) | ("stackalloc", [_]) => {
                let full = class.map(|c| self.l.u.full_name(c));
                let any =
                    ["scala.Any", "scala.Nothing", "scala.Null"].contains(&full.as_deref().unwrap_or(""));
                if any || class.is_none() {
                    return unsupported(span, "`stackalloc` of a type that is not concrete");
                }
                let ty = self.exact_type(tpe, span)?;
                let n = match args {
                    [size] => self.unboxed_size(*size)?,
                    _ => self.val(Val::Size(1)),
                };
                self.emit(Op::Stackalloc { ty, n })
            }
            _ => return Ok(None),
        };
        Ok(Some(v))
    }

    /// The size `stackalloc`'s argument `t` stands for, as a `size` (nscplugin's
    /// `getUnboxedSize`).
    fn unboxed_size(&mut self, t: TNodeId) -> Result<ValId> {
        match self.l.tpd.tree(t).clone() {
            Tree::Literal { value: Constant::Int(n) } => return Ok(self.val(Val::Size(n as i64 as u64))),
            Tree::Block { stats, expr } if stats.is_empty() => return self.unboxed_size(expr),
            Tree::Typed { expr, .. } => return self.unboxed_size(expr),
            Tree::Apply { fun, args }
                if let &[arg] = self.l.tpd.items(args)
                    && (self.is_size_conversion(fun) || self.is_size_conversion(arg)) =>
            {
                return self.unboxed_size(arg);
            }
            _ => {}
        }
        let size = self.gen_expr(t)?;
        let ty = self.val_ty(size);
        let integer = |t: &Type| matches!(t, Type::Byte | Type::Short | Type::Int | Type::Long | Type::Size);
        let (value, value_ty) = match self.pool.ty(ty).clone() {
            Type::Ref { name, .. } if let Some(u) = unboxed(self.pool.top_name(name)).filter(integer) => {
                let box_ty = self.ty(Type::Ref { name, exact: false, nullable: true });
                (self.emit(Op::Unbox { ty: box_ty, obj: size }), u)
            }
            t if integer(&t) => (size, t),
            _ => return unsupported(self.l.tpd.span(t), "`stackalloc` of a size that is no integer"),
        };
        if value_ty == Type::Size {
            return Ok(value);
        }
        let ty = self.ty(Type::Size);
        Ok(self.emit(Op::Conv { conv: Conv::SSizeCast, ty, value }))
    }

    /// Whether `t` is, or calls, a conversion between integers and sizes.
    fn is_size_conversion(&mut self, t: TNodeId) -> bool {
        let mut t = t;
        while let Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } = *self.l.tpd.tree(t) {
            t = fun;
        }
        let Some(s) = self.l.tpd.sym(t) else { return false };
        let owner = self.l.u.symbol(s).owner.map(|o| self.l.u.full_name(o));
        let name = self.l.u.symbol(s).name.as_str();
        SIZE_CONVERSIONS.iter().any(|(o, names)| owner.as_deref() == Some(o) && names.contains(&name))
    }
}
