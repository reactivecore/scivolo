// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! Operations of the primitive classes: unary operations, `&&` and `||`, conversions, and the
//! dispatch to binary ones (nscplugin's `genSimpleOp`, `genUnaryOp`).

use super::Gen;
use super::binary_ops::is_binary_op;
use super::callee::is_primitive;
use super::numeric::{float, int};
use crate::{Result, unsupported};
use names::nme;
use nir::{Bin, Op, Type, TypeId, Val, ValId};
use syntax::ast::constant::Constant;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// The operation `app` calls if `sym` is an operation of a primitive class; `None` for
    /// another method.
    pub fn gen_primitive(
        &mut self,
        app: TNodeId,
        sym: SymbolId,
        fun: TNodeId,
        args: &[TNodeId],
    ) -> Result<Option<ValId>> {
        if let Some(v) = self.gen_array_op(app, fun, args)? {
            return Ok(Some(v));
        }
        if let Some(v) = self.gen_intrinsic(app, sym, fun, args)? {
            return Ok(Some(v));
        }
        let Some(owner) = self.l.u.symbol(sym).owner else { return Ok(None) };
        let name = self.l.u.symbol(sym).name.clone();
        let defn = self.l.u.defn();
        let (any, object) = (defn.any, defn.object);
        if sym == self.l.defs.synchronized {
            return self.gen_synchronized(fun, args).map(Some);
        }
        let object_op = [any, object].contains(&owner)
            && matches!(name.as_str(), nme::REF_EQ | nme::REF_NE | nme::EQ | nme::NE | nme::HASHHASH);
        if !object_op && !is_primitive(&self.l.u.binary_name(owner)) {
            return Ok(None);
        }
        let Tree::Select { qualifier, .. } = *self.l.tpd.tree(fun) else { return Ok(None) };
        let span = self.l.tpd.span(app);
        if object_op {
            let operands = std::iter::once(qualifier).chain(args.iter().copied());
            for t in operands {
                let erased = crate::phases::adapt::erased_type(self.l, t).full();
                let ty = self.erased(&erased);
                if !matches!(
                    self.pool.ty(ty),
                    Type::Ref { .. } | Type::Array { .. } | Type::Null | Type::Nothing
                ) {
                    return unsupported(
                        self.l.tpd.span(t),
                        "universal equality and hashes of primitive values",
                    );
                }
            }
            let v = match (name.as_str(), args) {
                (nme::HASHHASH, []) => self.gen_hash_code(qualifier, span)?,
                (op, [right]) => self.gen_class_equality(qualifier, *right, op, span)?,
                _ => return unsupported(span, format!("the operation {name} of `Any`")),
            };
            return Ok(Some(v));
        }
        let retty = self.result_type(sym);
        let v = match (name.as_str(), args) {
            (nme::UNARY_MINUS | nme::UNARY_PLUS | nme::UNARY_TILDE | nme::UNARY_NOT, []) => {
                self.gen_unary(&name, qualifier, retty)?
            }
            (nme::ZAND | nme::ZOR, [right]) => {
                let literal = |g: &mut Self, b| {
                    let tpe = g.l.tpd.tpe(*right);
                    g.l.tpd.node(
                        Tree::Literal { value: Constant::Boolean(b) },
                        syntax::ast::Span::NONE,
                        Some(tpe),
                        None,
                    )
                };
                let (thenp, elsep) = if name == nme::ZAND {
                    (*right, literal(self, false))
                } else {
                    (literal(self, true), *right)
                };
                self.gen_if(retty, qualifier, thenp, elsep)?
            }
            (_, [right]) if is_binary_op(&name) => self.gen_binary(&name, qualifier, *right, retty)?,
            (
                nme::TO_BYTE
                | nme::TO_SHORT
                | nme::TO_CHAR
                | nme::TO_INT
                | nme::TO_LONG
                | nme::TO_FLOAT
                | nme::TO_DOUBLE,
                [],
            ) => {
                let value = self.gen_expr(qualifier)?;
                let from = self.gen_type(self.l.tpd.tpe(qualifier));
                self.coerce(value, from, retty)
            }
            _ => return unsupported(span, format!("the primitive operation {name}")),
        };
        Ok(Some(v))
    }

    fn gen_unary(&mut self, name: &str, operand: TNodeId, ty: TypeId) -> Result<ValId> {
        let value = self.gen_expr(operand)?;
        let from = self.val_ty(value);
        let v = self.coerce(value, from, ty);
        let t = self.pool.ty(ty).clone();
        let num = |g: &mut Self, n: i64| {
            let v = match t {
                Type::Byte => Val::Byte(n as i8),
                Type::Short | Type::Char => Val::Short(n as i16),
                Type::Int => Val::Int(n as i32),
                Type::Long => Val::Long(n),
                Type::Float => Val::Float((n as f32).to_bits()),
                _ => Val::Double((n as f64).to_bits()),
            };
            g.val(v)
        };
        Ok(match (name, int(&t).is_some() && t != Type::Bool, float(&t).is_some()) {
            (nme::UNARY_PLUS, _, _) => v,
            (nme::UNARY_TILDE, true, _) => {
                let minus_one = num(self, -1);
                self.emit(Op::Bin { bin: Bin::Xor, ty, l: minus_one, r: v })
            }
            (nme::UNARY_MINUS, _, true) => {
                let minus_one = num(self, -1);
                self.emit(Op::Bin { bin: Bin::Fmul, ty, l: minus_one, r: v })
            }
            (nme::UNARY_MINUS, true, _) => {
                let zero = num(self, 0);
                self.emit(Op::Bin { bin: Bin::Isub, ty, l: zero, r: v })
            }
            _ => {
                let tru = self.val(Val::True);
                self.emit(Op::Bin { bin: Bin::Xor, ty, l: tru, r: v })
            }
        })
    }
}
