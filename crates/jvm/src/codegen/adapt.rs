// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! A value fitted to the slots its context takes (dotty's `adapt`).

use super::body::Body;
use super::descriptors::{NOTHING, class_descriptor};
use super::{Gen, unsupported};
use lower::Result;
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// Generates `t` leaving `slots` slots: a value dropped, a `BoxedUnit` for none, as many as
    /// asked for a tree that does not fall through or is of type `Nothing`.
    pub fn expr_as(&mut self, b: &mut Body, t: TNodeId, slots: i32) -> Result<()> {
        self.expr_adapted(b, t, slots, true)
    }

    /// Generates `t` as a value of descriptor `desc`: as `expr_as`, without the `athrow` after a
    /// join of type `Nothing` where `desc` is `Nothing$`.
    pub fn expr_expect(&mut self, b: &mut Body, t: TNodeId, desc: &str) -> Result<()> {
        self.expr_adapted(b, t, classfile::value_slots(desc), desc != NOTHING)
    }

    /// Generates `t` leaving `slots` slots; with `adapt`, an `if`, match, labeled block or `try`
    /// of type `Nothing` ends with an `athrow` after its last label.
    fn expr_adapted(&mut self, b: &mut Body, t: TNodeId, slots: i32, adapt: bool) -> Result<()> {
        let depth = b.code.stack();
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

    /// Whether `t`, or the value of block or typed expression `t`, is an `if`, a match, a labeled
    /// block or a `try`, whose value of type `Nothing` dotty adapts after its last label.
    fn is_join(&self, t: TNodeId) -> bool {
        match *self.l.tpd.tree(t) {
            Tree::Typed { expr, .. } | Tree::Block { expr, .. } => self.is_join(expr),
            Tree::If { .. } | Tree::Match { .. } | Tree::Labeled { .. } | Tree::Try { .. } => true,
            _ => false,
        }
    }
}
