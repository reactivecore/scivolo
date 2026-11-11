// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! Binary operations of the primitive classes: arithmetic, bits, shifts and comparisons
//! (nscplugin's `genBinaryOp`).

use super::Gen;
use super::numeric::{float, int};
use crate::{Result, unsupported};
use names::nme;
use nir::{Bin, Comp, Op, Type, TypeId, ValId};
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    /// Emits binary operation `name` of a primitive class on `left` and `right`, coerced to
    /// `retty`.
    pub fn gen_binary(&mut self, name: &str, left: TNodeId, right: TNodeId, retty: TypeId) -> Result<ValId> {
        let lty = self.gen_type(self.l.tpd.tpe(left));
        let rty = self.gen_type(self.l.tpd.tpe(right));
        let shift = matches!(name, nme::LSL | nme::ASR | nme::LSR);
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

/// Determines whether `name` names a binary operation of a primitive class.
pub fn is_binary_op(name: &str) -> bool {
    bin_op(name).is_some()
}

fn bin_op(name: &str) -> Option<BinOp> {
    Some(match name {
        nme::ADD => BinOp::Arith(Bin::Iadd, Bin::Fadd),
        nme::MINUS => BinOp::Arith(Bin::Isub, Bin::Fsub),
        nme::MUL => BinOp::Arith(Bin::Imul, Bin::Fmul),
        nme::DIV => BinOp::Arith(Bin::Sdiv, Bin::Fdiv),
        nme::MOD => BinOp::Arith(Bin::Srem, Bin::Frem),
        nme::OR => BinOp::Bits(Bin::Or),
        nme::AND => BinOp::Bits(Bin::And),
        nme::XOR => BinOp::Bits(Bin::Xor),
        nme::LSL => BinOp::Bits(Bin::Shl),
        nme::LSR => BinOp::Bits(Bin::Lshr),
        nme::ASR => BinOp::Bits(Bin::Ashr),
        nme::EQ => BinOp::Cmp(Comp::Ieq, Comp::Feq),
        nme::NE => BinOp::Cmp(Comp::Ine, Comp::Fne),
        nme::LT => BinOp::Cmp(Comp::Slt, Comp::Flt),
        nme::LE => BinOp::Cmp(Comp::Sle, Comp::Fle),
        nme::GT => BinOp::Cmp(Comp::Sgt, Comp::Fgt),
        nme::GE => BinOp::Cmp(Comp::Sge, Comp::Fge),
        _ => return None,
    })
}
