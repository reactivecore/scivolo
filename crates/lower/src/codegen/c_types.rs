// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenType.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/PostInlineNativeInterop.scala
//! The C layout of types: C structs and arrays as values, boxes as their values (nscplugin's
//! `genType` deconstructing value types, `genAnonymousStruct`, `genFixedSizeArray`).

use super::Gen;
use super::externs::unboxed;
use crate::{Result, unsupported};
use nir::{Type, TypeId};
use syntax::ast::Span;
use tasty::SymbolId;

impl Gen<'_, '_> {
    /// `t` widened and dealiased, opaque types replaced by their aliases (nscplugin's
    /// `DealiasTypeMapper`).
    fn widen_dealias(&mut self, mut t: tasty::TypeId) -> tasty::TypeId {
        for _ in 0..64 {
            let w = self.l.u.widen(t);
            let mut d = self.l.u.dealias(w);
            if let tasty::Type::Sym(_, s) = *self.l.u.types().get(d)
                && let Some(alias) = self.l.u.opaque_alias(s)
            {
                d = alias;
            }
            if d == t {
                break;
            }
            t = d;
        }
        t
    }

    /// The class of type `t` after dealiasing, none for an abstract type or a type parameter.
    pub fn layout_class(&mut self, t: tasty::TypeId) -> Option<SymbolId> {
        let t = self.widen_dealias(t);
        if let tasty::Type::Sym(_, s) = *self.l.u.types().get(t)
            && !self.l.u.is_class(s)
        {
            return None;
        }
        self.l.u.class_symbol(t)
    }

    /// The NIR type of `t` with C structs and arrays as values and boxes as their values.
    pub fn exact_type(&mut self, t: tasty::TypeId, span: Span) -> Result<TypeId> {
        let t = self.widen_dealias(t);
        let Some(class) = self.layout_class(t) else {
            return Ok(self.class_type("java.lang.Object", false));
        };
        let args = match self.l.u.types().get(t) {
            tasty::Type::Applied(_, args) => args.clone(),
            _ => Vec::new(),
        };
        let name = self.l.u.full_name(class);
        let binary = self.l.u.binary_name(class);
        if let Some(t) = self.struct_type(&binary) {
            return Ok(t);
        }
        if name.strip_prefix("scala.scalanative.unsafe.CStruct").is_some_and(|n| n.parse::<u8>().is_ok()) {
            let fields = args.iter().map(|&a| self.exact_type(a, span)).collect::<Result<Vec<_>>>()?;
            return Ok(self.ty(Type::StructValue(fields)));
        }
        if name == "scala.scalanative.unsafe.CArray" {
            let (&[elem, _], Some(n)) = (&args[..], args.get(1).and_then(|&s| self.nat(s))) else {
                return unsupported(span, "a `CArray` of a size that is no natural number");
            };
            let elem = self.exact_type(elem, span)?;
            return Ok(self.ty(Type::ArrayValue(elem, n)));
        }
        let ty = self.gen_type(t);
        Ok(match self.pool.ty(ty).clone() {
            Type::Ref { name, .. } => match unboxed(self.pool.top_name(name)) {
                Some(u) => self.ty(u),
                None => ty,
            },
            _ => ty,
        })
    }

    /// The number type `t` of `unsafe.Nat` stands for.
    fn nat(&mut self, t: tasty::TypeId) -> Option<u32> {
        let t = self.widen_dealias(t);
        let class = self.l.u.class_symbol(t)?;
        let name = self.l.u.binary_name(class);
        let digit = |n: &str| {
            n.strip_prefix("scala.scalanative.unsafe.Nat$_")?.parse::<u32>().ok().filter(|&d| d < 10)
        };
        if let Some(d) = digit(&name) {
            return Some(d);
        }
        name.strip_prefix("scala.scalanative.unsafe.Nat$Digit")?;
        let args = self.l.u.applied(t)?.1.to_vec();
        args.into_iter().try_fold(0, |acc, a| Some(acc * 10 + self.nat(a)?))
    }
}
