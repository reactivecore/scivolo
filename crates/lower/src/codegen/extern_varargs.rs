// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenType.scala
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala

use super::Gen;
use crate::{Result, unsupported};
use nir::{Conv, Op, TypeId, ValId};
use syntax::ast::Span;
use tasty::{Erased, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    pub fn extern_vararg_index(&mut self, method: SymbolId) -> Option<usize> {
        let mut t = self.l.u.info(method);
        let mut index = 0;
        loop {
            match self.l.u.types().get(t).clone() {
                Type::Poly { result, .. } => t = result,
                Type::Method { params, result, .. } => {
                    for (_, param) in params {
                        if self.l.u.is_erased_param(param) {
                            continue;
                        }
                        let is_repeated = match *self.l.u.types().get(param) {
                            Type::Applied(c, _) => self
                                .l
                                .u
                                .class_symbol(c)
                                .is_some_and(|s| self.l.u.full_name(s) == "scala.<repeated>"),
                            Type::Annotated(_, a) => self.l.u.class_symbol(a).is_some_and(|s| {
                                self.l.u.full_name(s) == "scala.annotation.internal.Repeated"
                            }),
                            _ => false,
                        };
                        if is_repeated {
                            return Some(index);
                        }
                        index += 1;
                    }
                    t = result;
                }
                _ => return None,
            }
        }
    }

    /// The values of `arg`, the variable arguments of an extern call: each element unboxed to its
    /// type before erasure, made its C type and promoted as C promotes them (nscplugin's
    /// `genExternMethodArgs`).
    pub fn extern_varargs(&mut self, arg: TNodeId, span: Span) -> Result<Vec<ValId>> {
        let Some(seq) = self.seq_literal(arg) else {
            return unsupported(span, "variable arguments of an extern method not passed directly");
        };
        let Tree::SeqLiteral { elems, .. } = *self.l.tpd.tree(seq) else {
            unreachable!("a sequence literal")
        };
        let elems = self.l.tpd.items(elems).to_vec();
        let types = self.l.non_erased.get(&seq).map(|(types, _)| types.clone()).unwrap_or_default();
        if types.len() != elems.len() {
            return unsupported(span, "variable arguments of an extern method without their types");
        }
        let mut values = Vec::new();
        for (e, tpe) in elems.into_iter().zip(types) {
            let saved = self.at_span(self.l.tpd.span(e));
            let v = self.gen_expr(e)?;
            let erased = self.l.u.erase(tpe);
            let value_ty = self.value_type(tpe);
            let boxed = self.boxed_class(value_ty).filter(|_| self.val_ty(v) != value_ty);
            let to = boxed.unwrap_or(value_ty);
            let v = self.ensure_unboxed(v, to);
            let extern_ty = self.extern_type(&erased);
            let v = self.extern_arg(extern_ty, v);
            let unsigned =
                matches!(&erased, Erased::Class(n) if n.starts_with("scala.scalanative.unsigned.U"));
            values.push(self.promoted(v, unsigned));
            self.m.pos = saved;
        }
        Ok(values)
    }

    /// `v` promoted as a variable argument of C: a `float` to `double`, a narrower integer to
    /// `int`.
    fn promoted(&mut self, v: ValId, unsigned: bool) -> ValId {
        let ty = self.val_ty(v);
        let conv = match self.pool.ty(ty) {
            nir::Type::Float => Conv::Fpext,
            nir::Type::Byte | nir::Type::Short | nir::Type::Char if unsigned => Conv::Zext,
            nir::Type::Byte | nir::Type::Short | nir::Type::Char => Conv::Sext,
            _ => return v,
        };
        let to = if matches!(conv, Conv::Fpext) { nir::Type::Double } else { nir::Type::Int };
        let ty: TypeId = self.ty(to);
        self.emit(Op::Conv { conv, ty, value: v })
    }
}
