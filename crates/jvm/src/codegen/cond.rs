// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Conditional jumps (dotty's `genCond`, `genCJUMP`, `genCZJUMP`).

use super::method::Body;
use super::{Gen, unsupported};
use classfile::Label;
use lower::Result;
use syntax::ast::constant::Constant;
use tasty::Erased;
use typer::tpd::{TNodeId, Tree};

/// A comparison, in the order of the `ifeq` family of opcodes.
#[derive(Clone, Copy, PartialEq)]
pub enum Test {
    Eq,
    Ne,
    Lt,
    Ge,
    Gt,
    Le,
}

impl Test {
    pub fn of(name: &str) -> Option<Test> {
        Some(match name {
            "==" => Test::Eq,
            "!=" => Test::Ne,
            "<" => Test::Lt,
            ">=" => Test::Ge,
            ">" => Test::Gt,
            "<=" => Test::Le,
            _ => return None,
        })
    }

    fn negate(self) -> Test {
        [Test::Ne, Test::Eq, Test::Ge, Test::Lt, Test::Le, Test::Gt][self as usize]
    }
}

impl Gen<'_, '_> {
    /// Jumps to `success` if boolean `t` holds, else to `failure`; falls through to `fall`.
    pub fn cond(
        &mut self,
        b: &mut Body,
        t: TNodeId,
        success: Label,
        failure: Label,
        fall: Label,
    ) -> Result<()> {
        if let Some((name, receiver, args)) = self.primitive_call(t) {
            match (name.as_str(), &args[..]) {
                ("unary_!", []) => return self.cond(b, receiver, failure, success, fall),
                ("&&" | "||", &[right]) => {
                    let next = b.code.new_label();
                    match name.as_str() {
                        "&&" => self.cond(b, receiver, next, failure, next)?,
                        _ => self.cond(b, receiver, success, next, next)?,
                    }
                    b.code.place(next);
                    return self.cond(b, right, success, failure, fall);
                }
                (op, &[right]) if let Some(test) = Test::of(op) => {
                    let kind = self.operation_kind(receiver, right);
                    let mixed = kind == 'L' && (self.kind_of(receiver) != 'L' || self.kind_of(right) != 'L');
                    if mixed || kind == 'L' && !matches!(test, Test::Eq | Test::Ne) {
                        return unsupported(self.l.tpd.span(t), "an ordering of references");
                    }
                    let is_null =
                        |g: &Self, x| matches!(g.l.tpd.tree(x), Tree::Literal { value: Constant::Null });
                    let non_null = match (is_null(self, receiver), is_null(self, right)) {
                        (true, _) => Some(right),
                        (_, true) => Some(receiver),
                        _ => None,
                    };
                    if kind == 'L'
                        && let Some(x) = non_null
                    {
                        self.expr(b, x)?;
                        jump_zero(b, true, test, success, failure, fall);
                        return Ok(());
                    }
                    self.operand(b, receiver, kind)?;
                    self.operand(b, right, kind)?;
                    jump_compare(b, kind, test, success, failure, fall, false);
                    return Ok(());
                }
                _ => {}
            }
        }
        match *self.l.tpd.tree(t) {
            Tree::Block { stats, expr } => {
                for s in self.l.tpd.items(stats).to_vec() {
                    self.stat(b, s)?;
                }
                return self.cond(b, expr, success, failure, fall);
            }
            Tree::If { cond, thenp, elsep, .. } => {
                let (inner_success, inner_failure) = (b.code.new_label(), b.code.new_label());
                self.cond(b, cond, inner_success, inner_failure, inner_success)?;
                b.code.place(inner_success);
                self.cond(b, thenp, success, failure, inner_failure)?;
                b.code.place(inner_failure);
                return self.cond(b, elsep, success, failure, fall);
            }
            _ => {}
        }
        self.expr(b, t)?;
        jump_zero(b, false, Test::Ne, success, failure, fall);
        Ok(())
    }
}

/// The jump on comparing two values of kind `kind` (`I J F D L`) by `test`.
fn jump_compare(
    b: &mut Body,
    kind: char,
    test: Test,
    success: Label,
    failure: Label,
    fall: Label,
    negated: bool,
) {
    if fall == success {
        return jump_compare(b, kind, test.negate(), failure, success, fall, !negated);
    }
    let cmpg = match negated {
        true => matches!(test, Test::Gt | Test::Ge),
        false => matches!(test, Test::Lt | Test::Le),
    };
    match kind {
        'I' => b.code.jump(0x9f + test as u8, success),
        'L' => b.code.jump(0xa5 + test as u8, success),
        _ => {
            b.code.insn(match kind {
                'J' => 0x94,
                'F' => 0x95 + u8::from(cmpg),
                _ => 0x97 + u8::from(cmpg),
            });
            b.code.jump(0x99 + test as u8, success);
        }
    }
    if fall != failure {
        b.code.jump(0xa7, failure);
    }
}

/// The jump on comparing an int with zero, or a reference (`Eq` or `Ne` only) with null.
fn jump_zero(b: &mut Body, reference: bool, test: Test, success: Label, failure: Label, fall: Label) {
    if fall == success {
        return jump_zero(b, reference, test.negate(), failure, success, fall);
    }
    let opcode = match reference {
        true => 0xc6 + u8::from(test == Test::Ne),
        false => 0x99 + test as u8,
    };
    b.code.jump(opcode, success);
    if fall != failure {
        b.code.jump(0xa7, failure);
    }
}

/// The kind of erased primitive `e` in operations: `I J F D`, or `L` for a reference.
pub fn kind(e: &Erased) -> char {
    match e {
        Erased::Prim('j') => 'J',
        Erased::Prim('f') => 'F',
        Erased::Prim('d') => 'D',
        Erased::Prim(_) => 'I',
        _ => 'L',
    }
}
