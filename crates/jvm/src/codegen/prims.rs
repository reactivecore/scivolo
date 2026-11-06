// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeIdiomatic.scala
//! Operations of the primitive classes and conversions (dotty's `genPrimitiveOp`,
//! `genArithmeticOp`, `genConversion`, `emitT2T`).

use super::body::Body;
use super::cond::{Test, kind};
use super::{Gen, unsupported};
use lower::Result;
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
            && matches!(name.as_str(), "eq" | "ne" | "==" | "!=");
        if !PRIMITIVES.contains(&owner.as_str()) && !reference {
            return None;
        }
        Some((name, qualifier, self.l.tpd.items(args).to_vec()))
    }

    /// The kind both operands of a binary operation are converted to (dotty's `maxType`).
    pub fn operation_kind(&mut self, left: TNodeId, right: TNodeId) -> char {
        let (l, r) = (self.kind_of(left), self.kind_of(right));
        ['L', 'D', 'F', 'J', 'I'].into_iter().find(|&k| l == k || r == k).unwrap_or('I')
    }

    pub fn kind_of(&mut self, t: TNodeId) -> char {
        kind(&self.l.u.erase(self.l.tpd.tpe(t)))
    }

    /// Loads `t` converted to kind `to`.
    pub fn operand(&mut self, b: &mut Body, t: TNodeId, to: char) -> Result<()> {
        match (self.l.tpd.tree(t), to) {
            (&Tree::Literal { value: Constant::Int(v) }, 'J') => b.code.long(b.pool, v.into()),
            (&Tree::Literal { value: Constant::Float(v) }, 'D') => b.code.double(b.pool, v.into()),
            _ => {
                self.expr(b, t)?;
                let from = self.desc_of(t);
                convert(b, from.as_bytes()[0], to as u8);
            }
        }
        Ok(())
    }

    /// Call `t` of a primitive operation; `None` for another call.
    pub fn primitive(&mut self, b: &mut Body, t: TNodeId) -> Result<Option<i32>> {
        let Some((name, receiver, args)) = self.primitive_call(t) else { return Ok(None) };
        let result = self.desc_of(t);
        let to = result.as_bytes()[0];
        let k = |c: u8| u8::from(c == b'J') + 2 * u8::from(c == b'F') + 3 * u8::from(c == b'D');
        if name.starts_with("to") {
            return self.conversion(b, &name, receiver, &args, to);
        }
        let logical = matches!(name.as_str(), "unary_!" | "&&" | "||") || Test::of(&name).is_some();
        match (name.as_str(), &args[..]) {
            _ if logical => {
                let (success, failure, after) = (b.code.new_label(), b.code.new_label(), b.code.new_label());
                self.cond(b, t, success, failure, success)?;
                b.code.place(success);
                b.code.insn(0x04);
                b.code.jump(0xa7, after);
                b.code.place(failure);
                b.code.set_stack(b.code.stack() - 1);
                b.code.insn(0x03);
                b.code.place(after);
            }
            ("unary_+", []) => self.operand(b, receiver, to as char)?,
            ("unary_-", []) => {
                self.operand(b, receiver, to as char)?;
                b.code.insn(0x74 + k(to));
            }
            ("unary_~", []) => {
                self.operand(b, receiver, to as char)?;
                match to {
                    b'J' => b.code.long(b.pool, -1),
                    _ => b.code.insn(0x02),
                }
                b.code.insn(0x82 + k(to));
            }
            (op, &[right]) => {
                let shift = matches!(op, "<<" | ">>" | ">>>");
                let opk = match shift {
                    true if self.kind_of(receiver) == 'J' => 'J',
                    true => 'I',
                    false => self.operation_kind(receiver, right),
                };
                self.operand(b, receiver, opk)?;
                self.operand(b, right, if shift { 'I' } else { opk })?;
                let base = match op {
                    "+" => 0x60,
                    "-" => 0x64,
                    "*" => 0x68,
                    "/" => 0x6c,
                    "%" => 0x70,
                    "<<" => 0x78,
                    ">>" => 0x7a,
                    ">>>" => 0x7c,
                    "&" => 0x7e,
                    "|" => 0x80,
                    "^" => 0x82,
                    _ => return unsupported(self.l.tpd.span(t), format!("the primitive operation {name}")),
                };
                if opk == 'L' || base >= 0x78 && matches!(opk, 'F' | 'D') {
                    return unsupported(self.l.tpd.span(t), "bit operations on floats");
                }
                b.code.insn(base + k(opk as u8));
                convert(b, opk as u8, to);
            }
            _ => return unsupported(self.l.tpd.span(t), format!("the primitive operation {name}")),
        }
        Ok(Some(classfile::value_slots(&result)))
    }

    fn conversion(
        &mut self,
        b: &mut Body,
        name: &str,
        receiver: TNodeId,
        args: &[TNodeId],
        to: u8,
    ) -> Result<Option<i32>> {
        let known = ["toByte", "toShort", "toChar", "toInt", "toLong", "toFloat", "toDouble"];
        if !args.is_empty() || !known.contains(&name) {
            return unsupported(self.l.tpd.span(receiver), format!("the primitive operation {name}"));
        }
        self.expr(b, receiver)?;
        let from = self.desc_of(receiver);
        convert(b, from.as_bytes()[0], to);
        Ok(Some(classfile::value_slots(&(to as char).to_string())))
    }
}

/// Converts the value on the stack from descriptor letter `from` to `to` (dotty's `emitT2T`).
pub fn convert(b: &mut Body, from: u8, to: u8) {
    let wide = |c: u8| match c {
        b'Z' | b'B' | b'C' | b'S' | b'I' => b'I',
        c => c,
    };
    let (f, t) = (wide(from), wide(to));
    if f == b'L' || t == b'L' || f == b'[' || t == b'[' {
        return;
    }
    let row = |c: u8| b"IJFD".iter().position(|&x| x == c);
    if f != t
        && let (Some(i), Some(j)) = (row(f), row(t))
    {
        let index =
            [[0, 0x85, 0x86, 0x87], [0x88, 0, 0x89, 0x8a], [0x8b, 0x8c, 0, 0x8d], [0x8e, 0x8f, 0x90, 0]];
        b.code.insn(index[i][j]);
    }
    let narrow = match to {
        b'B' if from != b'B' => 0x91,
        b'C' if from != b'C' => 0x92,
        b'S' if !matches!(from, b'S' | b'B') => 0x93,
        _ => return,
    };
    b.code.insn(narrow);
}
