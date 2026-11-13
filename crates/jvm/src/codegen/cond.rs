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

#[cfg(test)]
mod tests {
    use super::*;
    use classfile::{IFNE, IFNONNULL, NOP, Pool};

    const IFLT: u8 = 0x9b;
    const IFGE: u8 = 0x9c;
    const IFGT: u8 = 0x9d;
    const IFLE: u8 = 0x9e;
    const IF_ICMPLT: u8 = 0xa1;
    const IF_ICMPGE: u8 = 0xa2;
    const IF_ACMPNE: u8 = 0xa6;
    const FCMPG: u8 = 0x96;
    const DCMPG: u8 = 0x98;

    /// Where the code after a conditional jump goes.
    #[derive(Clone, Copy)]
    enum Fall {
        Success,
        Failure,
        Neither,
    }

    /// The bytes `emit` writes on a stack of `depth` slots, given the labels success, failure and
    /// the one `fall` names; failure is placed right after them, success one `nop` later.
    fn jumps(depth: i32, fall: Fall, emit: impl FnOnce(&mut Body, Label, Label, Label)) -> Vec<u8> {
        let mut pool = Pool::default();
        let mut b = Body::new(&mut pool, None, 0);
        b.code.set_stack(depth);
        let (success, failure) = (b.code.new_label(), b.code.new_label());
        let fall = match fall {
            Fall::Success => success,
            Fall::Failure => failure,
            Fall::Neither => b.code.new_label(),
        };
        emit(&mut b, success, failure, fall);
        let n = b.code.bytes.len();
        b.code.place(failure);
        b.code.insn(NOP);
        b.code.place(success);
        b.code.bytes.truncate(n);
        b.code.bytes
    }

    /// The bytes of comparing two values of kind `kind` by `test`.
    fn compare(kind: Kind, test: Test, fall: Fall) -> Vec<u8> {
        jumps(2 * kind.slots(), fall, |b, s, f, fall| jump_compare(b, kind, test, s, f, fall, false))
    }

    #[test]
    fn tests_of_names_and_their_negations() {
        let table = [
            ("==", Test::Eq, Test::Ne),
            ("eq", Test::Eq, Test::Ne),
            ("!=", Test::Ne, Test::Eq),
            ("ne", Test::Ne, Test::Eq),
            ("<", Test::Lt, Test::Ge),
            (">=", Test::Ge, Test::Lt),
            (">", Test::Gt, Test::Le),
            ("<=", Test::Le, Test::Gt),
        ];
        for (name, test, negated) in table {
            assert!(Test::of(name) == Some(test), "{name}");
            assert!(test.negate() == negated && negated.negate() == test, "{name}");
        }
        assert!(Test::of("+").is_none() && Test::of("equals").is_none());
    }

    #[test]
    fn jumps_on_zero_and_null() {
        let zero =
            |reference, test, fall| jumps(1, fall, |b, s, f, fall| jump_zero(b, reference, test, s, f, fall));
        assert_eq!(zero(false, Test::Eq, Fall::Failure), [IFEQ, 0, 4]);
        assert_eq!(zero(false, Test::Eq, Fall::Success), [IFNE, 0, 3]);
        assert_eq!(zero(false, Test::Lt, Fall::Neither), [IFLT, 0, 7, GOTO, 0, 3]);
        assert_eq!(zero(true, Test::Eq, Fall::Failure), [IFNULL, 0, 4]);
        assert_eq!(zero(true, Test::Ne, Fall::Failure), [IFNONNULL, 0, 4]);
        assert_eq!(zero(true, Test::Eq, Fall::Success), [IFNONNULL, 0, 3]);
    }

    #[test]
    fn comparisons_per_kind_and_fall_through() {
        assert_eq!(compare(Kind::Int, Test::Lt, Fall::Failure), [IF_ICMPLT, 0, 4]);
        assert_eq!(compare(Kind::Int, Test::Lt, Fall::Success), [IF_ICMPGE, 0, 3]);
        assert_eq!(compare(Kind::Reference, Test::Ne, Fall::Failure), [IF_ACMPNE, 0, 4]);
        assert_eq!(compare(Kind::Long, Test::Gt, Fall::Failure), [LCMP, IFGT, 0, 4]);
        assert_eq!(compare(Kind::Long, Test::Eq, Fall::Neither), [LCMP, IFEQ, 0, 7, GOTO, 0, 3]);
    }

    #[test]
    fn float_comparisons_are_false_on_nan() {
        assert_eq!(compare(Kind::Float, Test::Lt, Fall::Failure), [FCMPG, IFLT, 0, 4]);
        assert_eq!(compare(Kind::Float, Test::Lt, Fall::Success), [FCMPG, IFGE, 0, 3]);
        assert_eq!(compare(Kind::Float, Test::Ge, Fall::Success), [FCMPL, IFLT, 0, 3]);
        assert_eq!(compare(Kind::Double, Test::Gt, Fall::Failure), [DCMPL, IFGT, 0, 4]);
        assert_eq!(compare(Kind::Double, Test::Gt, Fall::Success), [DCMPL, IFLE, 0, 3]);
        assert_eq!(compare(Kind::Double, Test::Le, Fall::Neither), [DCMPG, IFLE, 0, 7, GOTO, 0, 3]);
    }

    #[test]
    fn operation_kinds_of_erased_types() {
        for (c, k) in
            [('z', Kind::Int), ('c', Kind::Int), ('b', Kind::Int), ('s', Kind::Int), ('i', Kind::Int)]
        {
            assert_eq!(kind(&Erased::Prim(c)), k);
        }
        assert_eq!(kind(&Erased::Prim('j')), Kind::Long);
        assert_eq!(kind(&Erased::Prim('f')), Kind::Float);
        assert_eq!(kind(&Erased::Prim('d')), Kind::Double);
        assert_eq!(kind(&Erased::Class("java.lang.Integer".into())), Kind::Reference);
        assert_eq!(kind(&Erased::Array(Box::new(Erased::Prim('i')))), Kind::Reference);
    }
}
