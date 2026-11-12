// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeIdiomatic.scala
//! Operations of the primitive classes and conversions (dotty's `genPrimitiveOp`,
//! `genArithmeticOp`, `genConversion`, `emitT2T`).

use super::body::{Body, Operand};
use super::cond::{Test, kind};
use super::{Gen, unsupported};
use classfile::{
    CONVERSIONS, GOTO, I2B, I2C, I2S, IADD, IAND, ICONST_0, ICONST_1, ICONST_M1, IDIV, IMUL, INEG, IOR, IREM,
    ISHL, ISHR, ISUB, IUSHR, IXOR, Kind,
};
use lower::Result;
use names::nme;
use syntax::ast::constant::Constant;
use typer::tpd::{TNodeId, Tree};

const PRIMITIVES: [&str; 8] = [
    "scala.Boolean",
    "scala.Char",
    "scala.Byte",
    "scala.Short",
    "scala.Int",
    "scala.Long",
    "scala.Float",
    "scala.Double",
];

impl Gen<'_, '_> {
    /// The name, receiver and arguments of `t` if it calls an operation of a primitive class, or
    /// `eq`, `ne`, `==` or `!=` of `Any`.
    pub fn primitive_call(&mut self, t: TNodeId) -> Option<(String, TNodeId, Vec<TNodeId>)> {
        let Tree::Apply { fun, args } = *self.l.tpd.tree(t) else { return None };
        let Tree::Select { qualifier, .. } = *self.l.tpd.tree(fun) else { return None };
        let m = self.l.tpd.sym(fun)?;
        let owner = self.l.u.binary_name(self.l.u.symbol(m).owner?);
        let name = self.l.u.symbol(m).name.clone();
        let reference = matches!(owner.as_str(), "scala.Any" | "scala.AnyRef" | "java.lang.Object")
            && matches!(name.as_str(), nme::REF_EQ | nme::REF_NE | nme::EQ | nme::NE);
        if !PRIMITIVES.contains(&owner.as_str()) && !reference {
            return None;
        }
        Some((name, qualifier, self.l.tpd.items(args).to_vec()))
    }

    /// The kind both operands of a binary operation are converted to (dotty's `maxType`).
    pub fn operation_kind(&mut self, left: TNodeId, right: TNodeId) -> Kind {
        let (l, r) = (self.kind_of(left), self.kind_of(right));
        [Kind::Reference, Kind::Double, Kind::Float, Kind::Long]
            .into_iter()
            .find(|&k| l == k || r == k)
            .unwrap_or(Kind::Int)
    }

    pub fn kind_of(&mut self, t: TNodeId) -> Kind {
        kind(&lower::erased_type(self.l, t))
    }

    /// Loads `t` converted to kind `to`.
    pub fn operand(&mut self, b: &mut Body, t: TNodeId, to: Kind) -> Result<()> {
        match (self.l.tpd.tree(t), to) {
            (&Tree::Literal { value: Constant::Int(v) }, Kind::Long) => b.code.long(b.pool, v.into()),
            (&Tree::Literal { value: Constant::Float(v) }, Kind::Double) => b.code.double(b.pool, v.into()),
            _ => {
                self.expr(b, t)?;
                let from = Kind::of(&self.desc_of(t));
                convert(b, from, to);
            }
        }
        Ok(())
    }

    /// Call `t` of a primitive operation; `None` for another call.
    pub fn primitive(&mut self, b: &mut Body, t: TNodeId) -> Result<Option<i32>> {
        let Some((name, receiver, args)) = self.primitive_call(t) else { return Ok(None) };
        if self.is_concat(t) {
            return Ok(None);
        }
        let result = self.desc_of(t);
        let to = Kind::of(&result);
        if name.starts_with("to") {
            return self.conversion(b, &name, receiver, &args, to);
        }
        let logical =
            matches!(name.as_str(), nme::UNARY_NOT | nme::ZAND | nme::ZOR) || Test::of(&name).is_some();
        match (name.as_str(), &args[..]) {
            _ if logical => {
                let (success, failure, after) = (b.code.new_label(), b.code.new_label(), b.code.new_label());
                self.cond(b, t, success, failure, success)?;
                b.code.place(success);
                b.code.insn(ICONST_1);
                b.code.jump(GOTO, after);
                b.code.place(failure);
                b.code.set_stack(b.code.stack() - 1);
                b.code.insn(ICONST_0);
                b.code.place(after);
            }
            (nme::UNARY_MINUS | nme::UNARY_TILDE, []) if to.numeric_offset().is_none() => {
                return unsupported(self.l.tpd.span(t), format!("the primitive operation {name}"));
            }
            (nme::UNARY_PLUS, []) => self.operand(b, receiver, to)?,
            (nme::UNARY_MINUS, []) => {
                self.operand(b, receiver, to)?;
                b.code.insn(INEG + to.offset());
            }
            (nme::UNARY_TILDE, []) => {
                self.operand(b, receiver, to)?;
                match to {
                    Kind::Long => b.code.long(b.pool, -1),
                    _ => b.code.insn(ICONST_M1),
                }
                b.code.insn(IXOR + to.offset());
            }
            (op, &[right]) => {
                let shift = matches!(op, nme::LSL | nme::ASR | nme::LSR);
                let opk = match shift {
                    true if self.kind_of(receiver) == Kind::Long => Kind::Long,
                    true => Kind::Int,
                    false => self.operation_kind(receiver, right),
                };
                self.operand(b, receiver, opk)?;
                self.operand(b, right, if shift { Kind::Int } else { opk })?;
                let base = match op {
                    nme::ADD => IADD,
                    nme::MINUS => ISUB,
                    nme::MUL => IMUL,
                    nme::DIV => IDIV,
                    nme::MOD => IREM,
                    nme::LSL => ISHL,
                    nme::ASR => ISHR,
                    nme::LSR => IUSHR,
                    nme::AND => IAND,
                    nme::OR => IOR,
                    nme::XOR => IXOR,
                    _ => return unsupported(self.l.tpd.span(t), format!("the primitive operation {name}")),
                };
                if opk == Kind::Reference || base >= ISHL && matches!(opk, Kind::Float | Kind::Double) {
                    return unsupported(self.l.tpd.span(t), "bit operations on floats");
                }
                b.code.insn(base + opk.offset());
                self.line(b, t);
                convert(b, opk, to);
            }
            _ => return unsupported(self.l.tpd.span(t), format!("the primitive operation {name}")),
        }
        if name.starts_with(nme::UNARY_PREFIX) && name != nme::UNARY_NOT {
            self.line(b, t);
        }
        Ok(Some(classfile::value_slots(&result)))
    }

    fn conversion(
        &mut self,
        b: &mut Body,
        name: &str,
        receiver: TNodeId,
        args: &[TNodeId],
        to: Kind,
    ) -> Result<Option<i32>> {
        let known = [
            nme::TO_BYTE,
            nme::TO_SHORT,
            nme::TO_CHAR,
            nme::TO_INT,
            nme::TO_LONG,
            nme::TO_FLOAT,
            nme::TO_DOUBLE,
        ];
        if !args.is_empty() || !known.contains(&name) {
            return unsupported(self.l.tpd.span(receiver), format!("the primitive operation {name}"));
        }
        self.expr(b, receiver)?;
        let from = Kind::of(&self.desc_of(receiver));
        convert(b, from, to);
        Ok(Some(to.slots()))
    }
}

/// Converts the value on the stack from kind `from` to `to` (dotty's `emitT2T`).
pub fn convert(b: &mut Body, from: Kind, to: Kind) {
    let (f, t) = (from.widened(), to.widened());
    if f == Kind::Reference || t == Kind::Reference {
        return;
    }
    if f != t
        && let (Some(i), Some(j)) = (f.numeric_offset(), t.numeric_offset())
    {
        b.code.insn(CONVERSIONS[usize::from(i)][usize::from(j)]);
        b.operand(b.code.stack() - t.slots(), t.slots(), Operand::Desc(t.descriptor()));
    }
    let narrow = match to {
        Kind::Byte if from != Kind::Byte => I2B,
        Kind::Char if from != Kind::Char => I2C,
        Kind::Short if !matches!(from, Kind::Short | Kind::Byte) => I2S,
        _ => return,
    };
    b.code.insn(narrow);
}
