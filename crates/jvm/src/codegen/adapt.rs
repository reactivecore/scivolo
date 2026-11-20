// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! A value fitted to the slots its context takes (dotty's `adapt`).

use super::body::{Body, Operand};
use super::descriptors::{NOTHING, NULL, class_descriptor};
use super::{Gen, unsupported};
use lower::Result;
use tasty::{Erased, SymbolId};
use typer::tpd::{List, TNodeId, Tree};

impl Gen<'_, '_> {
    /// Generates `t` leaving `slots` slots: a value dropped, a `BoxedUnit` for none, as many as
    /// asked for a tree that does not fall through or is of type `Nothing`.
    pub fn expr_as(&mut self, b: &mut Body, t: TNodeId, slots: i32) -> Result<()> {
        self.expr_adapted(b, t, slots, true)
    }

    /// Generates `t` as a value of descriptor `desc`: as `expr_as`, without the `athrow` after a
    /// join of type `Nothing` where `desc` is `Nothing$`, a `Null$` kept where `desc` is `Null$`.
    pub fn expr_expect(&mut self, b: &mut Body, t: TNodeId, desc: &str) -> Result<()> {
        if desc == NULL {
            self.expect_null(b, t);
        }
        self.expr_adapted(b, t, classfile::value_slots(desc), desc != NOTHING)
    }

    /// Generates `t` leaving `slots` slots; with `adapt`, an `if`, match, labeled block or `try` of
    /// type `Nothing` ends with an `athrow` after its last label, which no jump reaches.
    fn expr_adapted(&mut self, b: &mut Body, t: TNodeId, slots: i32, adapt: bool) -> Result<()> {
        let depth = b.code.stack();
        if let Tree::Block { stats, expr } = *self.l.tpd.tree(t) {
            return self.block_adapted(b, t, (stats, expr), slots, adapt);
        }
        let got = self.expr(b, t)?;
        let nothing = got == 1 && self.desc_of(t) == NOTHING;
        match got - slots {
            _ if !b.code.live() => b.code.set_stack(depth + slots),
            0 if adapt && nothing && self.is_join(t) => {
                b.code.insn(classfile::ATHROW);
                b.code.set_stack(depth + slots);
            }
            0 => {}
            1 if slots == 0 => b.code.insn(classfile::POP),
            2 if slots == 0 => b.code.insn(classfile::POP2),
            -1 if got == 0 => {
                b.push("L");
                let unit = "scala/runtime/BoxedUnit";
                b.code.field(classfile::FieldOp::GetStatic, b.pool, unit, "UNIT", &class_descriptor(unit));
            }
            _ if nothing => {
                b.code.insn(classfile::ATHROW);
                b.code.set_stack(depth + slots);
            }
            _ => return unsupported(self.l.tpd.span(t), "a value of another size than its context's"),
        }
        b.forget(depth + slots);
        Ok(())
    }

    /// Generates block `t` of `stats` and `expr` with `expr` adapted as `expr_adapted` does within
    /// the scope of the block's locals (dotty's `genBlockTo`).
    fn block_adapted(
        &mut self,
        b: &mut Body,
        t: TNodeId,
        (stats, expr): (List, TNodeId),
        slots: i32,
        adapt: bool,
    ) -> Result<()> {
        let depth = b.code.stack();
        self.line(b, t);
        b.code.open_block();
        for s in self.l.tpd.items(stats).to_vec() {
            self.block_stat(b, s)?;
        }
        self.expr_adapted(b, expr, slots, adapt)?;
        b.code.close_block();

        if let Some(top) =
            b.operands.last_mut().filter(|o| o.0 == depth && matches!(o.2, Operand::Of(e) if e == expr))
        {
            top.2 = Operand::Of(t);
        }
        Ok(())
    }

    /// Whether `t`, or the value of block or typed expression `t`, is an `if`, a match, a `try` or
    /// a labeled block no `return` leaves whose erased type is `Nothing`, a value dotty adapts
    /// after its last label.
    fn is_join(&mut self, t: TNodeId) -> bool {
        match *self.l.tpd.tree(t) {
            Tree::Typed { expr, .. } | Tree::Block { expr, .. } => self.is_join(expr),
            Tree::If { .. } | Tree::Match { .. } | Tree::Try { .. } => self.erased_nothing(t),
            Tree::Labeled { expr } => {
                let label = self.l.tpd.sym(t);
                !self.returns_to(expr, label) && self.erased_nothing(expr)
            }
            _ => false,
        }
    }

    /// Whether `t` contains a `return` from `label` with a value of erased type other than
    /// `Nothing`.
    fn returns_to(&mut self, t: TNodeId, label: Option<SymbolId>) -> bool {
        if let Tree::Return { expr, from } = *self.l.tpd.tree(t)
            && self.l.tpd.sym(from) == label
            && !self.erased_nothing(expr)
        {
            return true;
        }
        self.l.tpd.children(t).into_iter().any(|c| self.returns_to(c, label))
    }

    /// Whether erasure leaves `t` of type `Nothing`: a throw, a return, a call of a method or a
    /// value of that erased type, or a join of such branches.
    fn erased_nothing(&mut self, t: TNodeId) -> bool {
        let branches = match *self.l.tpd.tree(t) {
            Tree::Typed { expr, .. } | Tree::Block { expr, .. } => vec![expr],
            Tree::If { thenp, elsep, .. } => vec![thenp, elsep],
            Tree::Match { cases, .. } => self.case_bodies(cases),
            Tree::Try { expr, cases, .. } => [vec![expr], self.case_bodies(cases)].concat(),
            Tree::Return { .. } => return true,
            Tree::Apply { fun, .. } if !self.is_throw(t) => {
                return match self.l.tpd.sym(fun) {
                    Some(m) => self.l.u.erased_method_value(m).1 == Erased::Nothing,
                    None => self.desc_of(t) == NOTHING,
                };
            }
            _ => return self.desc_of(t) == NOTHING,
        };
        branches.into_iter().all(|b| self.erased_nothing(b))
    }

    fn case_bodies(&self, cases: List) -> Vec<TNodeId> {
        let bodies = self.l.tpd.items(cases).iter().map(|&c| match *self.l.tpd.tree(c) {
            Tree::CaseDef { body, .. } => Some(body),
            _ => None,
        });
        bodies.collect::<Option<Vec<_>>>().unwrap_or_default()
    }
}
