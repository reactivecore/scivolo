// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Conditional jumps (dotty's `genCond`, `genCJUMP`, `genCZJUMP`).

use super::body::Body;
use super::{Gen, unsupported};
use classfile::{DCMPL, FCMPL, GOTO, IF_ACMPEQ, IF_ICMPEQ, IFEQ, IFNULL, Kind, LCMP, Label};
use lower::Result;
use names::nme;
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
            nme::EQ | nme::REF_EQ => Test::Eq,
            nme::NE | nme::REF_NE => Test::Ne,
            nme::LT => Test::Lt,
            nme::GE => Test::Ge,
            nme::GT => Test::Gt,
            nme::LE => Test::Le,
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
                (nme::UNARY_NOT, []) => return self.cond(b, receiver, failure, success, fall),
                (nme::ZAND | nme::ZOR, &[right]) => {
                    let next = b.code.new_label();
                    match name.as_str() {
                        nme::ZAND => self.cond(b, receiver, next, failure, next)?,
                        _ => self.cond(b, receiver, success, next, next)?,
                    }
                    b.code.place(next);
                    return self.cond(b, right, success, failure, fall);
                }
                (op, &[right]) if let Some(test) = Test::of(op) => {
                    let kind = self.operation_kind(receiver, right);
                    let reference = kind == Kind::Reference;
                    let mixed = reference
                        && (self.kind_of(receiver) != Kind::Reference
                            || self.kind_of(right) != Kind::Reference);
                    if mixed || reference && !matches!(test, Test::Eq | Test::Ne) {
                        return unsupported(self.l.tpd.span(t), "an ordering of references");
                    }
                    let universal = matches!(op, nme::EQ | nme::NE);
                    if reference
                        && universal
                        && !matches!(lower::erased_type(self.l, receiver), Erased::Array(_))
                    {
                        let labels = match test {
                            Test::Eq => [success, failure, fall],
                            _ => [failure, success, fall],
                        };
                        return self.eq_eq(b, (receiver, right), labels);
                    }
                    let is_null =
                        |g: &Self, x| matches!(g.l.tpd.tree(x), Tree::Literal { value: Constant::Null });
                    let non_null = match (is_null(self, receiver), is_null(self, right)) {
                        (true, _) => Some(right),
                        (_, true) => Some(receiver),
                        _ => None,
                    };
                    if reference
                        && !universal
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
                    self.block_stat(b, s)?;
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
        self.expr_as(b, t, 1)?;
        jump_zero(b, false, Test::Ne, success, failure, fall);
        Ok(())
    }
}

/// The jump on comparing two values of operation kind `kind` by `test`.
fn jump_compare(
    b: &mut Body,
    kind: Kind,
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
        Kind::Int => b.code.jump(IF_ICMPEQ + test as u8, success),
        Kind::Reference => b.code.jump(IF_ACMPEQ + test as u8, success),
        _ => {
            b.code.insn(match kind {
                Kind::Long => LCMP,
                Kind::Float => FCMPL + u8::from(cmpg),
                _ => DCMPL + u8::from(cmpg),
            });
            b.code.jump(IFEQ + test as u8, success);
        }
    }
    if fall != failure {
        b.code.jump(GOTO, failure);
    }
}

/// The jump on comparing an int with zero, or a reference (`Eq` or `Ne` only) with null.
pub fn jump_zero(b: &mut Body, reference: bool, test: Test, success: Label, failure: Label, fall: Label) {
    if fall == success {
        return jump_zero(b, reference, test.negate(), failure, success, fall);
    }
    let opcode = match reference {
        true => IFNULL + u8::from(test == Test::Ne),
        false => IFEQ + test as u8,
    };
    b.code.jump(opcode, success);
    if fall != failure {
        b.code.jump(GOTO, failure);
    }
}

/// The kind of erased type `e` in operations: `Int`, `Long`, `Float`, `Double` or `Reference`.
pub fn kind(e: &Erased) -> Kind {
    match e {
        Erased::Prim('j') => Kind::Long,
        Erased::Prim('f') => Kind::Float,
        Erased::Prim('d') => Kind::Double,
        Erased::Prim(_) => Kind::Int,
        _ => Kind::Reference,
    }
}
