//! Operations of the primitive classes: arithmetic, comparisons, logic, shifts, `&&` and `||`,
//! conversions (nscplugin's `genSimpleOp`, `genUnaryOp`, `genBinaryOp`).

use super::Gen;
use super::callee::is_primitive;
use super::numeric::{float, int};
use crate::{Result, unsupported};
use nir::{Bin, Comp, Op, Type, TypeId, Val, ValId};
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
        let Some(owner) = self.l.u.symbol(sym).owner else { return Ok(None) };
        if !is_primitive(&self.l.u.binary_name(owner)) {
            return Ok(None);
        }
        let Tree::Select { qualifier, .. } = *self.l.tpd.tree(fun) else { return Ok(None) };
        let name = self.l.u.symbol(sym).name.clone();
        let span = self.l.tpd.span(app);
        let retty = self.result_type(sym);
        let v = match (name.as_str(), args) {
            ("unary_-" | "unary_+" | "unary_~" | "unary_!", []) => self.gen_unary(&name, qualifier, retty)?,
            ("&&" | "||", [right]) => {
                let literal = |g: &mut Self, b| {
                    let tpe = g.l.tpd.tpe(*right);
                    g.l.tpd.node(
                        Tree::Literal { value: Constant::Boolean(b) },
                        syntax::ast::Span::NONE,
                        Some(tpe),
                        None,
                    )
                };
                let (thenp, elsep) =
                    if name == "&&" { (*right, literal(self, false)) } else { (literal(self, true), *right) };
                self.gen_if(retty, qualifier, thenp, elsep)?
            }
            (_, [right]) if bin_op(&name).is_some() => self.gen_binary(&name, qualifier, *right, retty)?,
            ("toByte" | "toShort" | "toChar" | "toInt" | "toLong" | "toFloat" | "toDouble", []) => {
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
            ("unary_+", _, _) => v,
            ("unary_~", true, _) => {
                let minus_one = num(self, -1);
                self.emit(Op::Bin { bin: Bin::Xor, ty, l: minus_one, r: v })
            }
            ("unary_-", _, true) => {
                let minus_one = num(self, -1);
                self.emit(Op::Bin { bin: Bin::Fmul, ty, l: minus_one, r: v })
            }
            ("unary_-", true, _) => {
                let zero = num(self, 0);
                self.emit(Op::Bin { bin: Bin::Isub, ty, l: zero, r: v })
            }
            _ => {
                let tru = self.val(Val::True);
                self.emit(Op::Bin { bin: Bin::Xor, ty, l: tru, r: v })
            }
        })
    }

    fn gen_binary(&mut self, name: &str, left: TNodeId, right: TNodeId, retty: TypeId) -> Result<ValId> {
        let lty = self.gen_type(self.l.tpd.tpe(left));
        let rty = self.gen_type(self.l.tpd.tpe(right));
        let shift = matches!(name, "<<" | ">>" | ">>>");
        let opty = if shift {
            self.ty(if *self.pool.ty(lty) == Type::Long { Type::Long } else { Type::Int })
        } else {
            self.operation_type(lty, rty)
        };
        let t = self.pool.ty(opty).clone();
        let op = bin_op(name).expect("a binary operation");
        let floating = float(&t).is_some();
        let op = match (op, floating) {
            (BinOp::Arith(i, f), _) => Ok(if floating { f } else { i }),
            (BinOp::Bits(b), false) => Ok(b),
            (BinOp::Cmp(i, f), _) => Err(if floating { f } else { i }),
            (BinOp::Bits(_), true) => return unsupported(self.l.tpd.span(right), "bit operations on floats"),
        };
        if int(&t).is_none() && !floating {
            return unsupported(self.l.tpd.span(right), "operations on references");
        }
        let saved = self.at_span(self.l.tpd.span(left));
        let l = self.gen_expr(left)?;
        let l = self.coerce(l, lty, opty);
        self.at_span(self.l.tpd.span(right));
        let r = self.gen_expr(right)?;
        let r = self.coerce(r, rty, opty);
        self.at_span(self.l.tpd.span(left));
        let v = match op {
            Ok(bin) => self.emit(Op::Bin { bin, ty: opty, l, r }),
            Err(comp) => self.emit(Op::Comp { comp, ty: opty, l, r }),
        };
        self.at_span(self.l.tpd.span(right));
        let from = self.val_ty(v);
        let v = self.coerce(v, from, retty);
        self.m.pos = saved;
        Ok(v)
    }
}

/// A binary operation: arithmetic for integers and floats, bits for integers, a comparison.
enum BinOp {
    Arith(Bin, Bin),
    Bits(Bin),
    Cmp(Comp, Comp),
}

fn bin_op(name: &str) -> Option<BinOp> {
    Some(match name {
        "+" => BinOp::Arith(Bin::Iadd, Bin::Fadd),
        "-" => BinOp::Arith(Bin::Isub, Bin::Fsub),
        "*" => BinOp::Arith(Bin::Imul, Bin::Fmul),
        "/" => BinOp::Arith(Bin::Sdiv, Bin::Fdiv),
        "%" => BinOp::Arith(Bin::Srem, Bin::Frem),
        "|" => BinOp::Bits(Bin::Or),
        "&" => BinOp::Bits(Bin::And),
        "^" => BinOp::Bits(Bin::Xor),
        "<<" => BinOp::Bits(Bin::Shl),
        ">>>" => BinOp::Bits(Bin::Lshr),
        ">>" => BinOp::Bits(Bin::Ashr),
        "==" => BinOp::Cmp(Comp::Ieq, Comp::Feq),
        "!=" => BinOp::Cmp(Comp::Ine, Comp::Fne),
        "<" => BinOp::Cmp(Comp::Slt, Comp::Flt),
        "<=" => BinOp::Cmp(Comp::Sle, Comp::Fle),
        ">" => BinOp::Cmp(Comp::Sgt, Comp::Fgt),
        ">=" => BinOp::Cmp(Comp::Sge, Comp::Fge),
        _ => return None,
    })
}
