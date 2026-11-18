// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Labeled blocks and returns (dotty's `genLabeledTo`, `genReturn`).

use super::body::{Body, Dest};
use super::callee::adapt_phantom;
use super::descriptors::NULL;
use super::{Gen, unsupported};
use lower::Result;
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    /// Labeled block `t` whose value stays on the stack, which `return[t]` leaves.
    pub fn gen_labeled(&mut self, b: &mut Body, t: TNodeId, expr: TNodeId) -> Result<i32> {
        let label = self.l.tpd.sym(t).expect("a label's symbol");
        let desc = self.desc_of(t);
        let slots = classfile::value_slots(&desc);
        let (end, depth) = (b.code.new_label(), b.code.stack());
        let adapt_join = desc == NULL && !b.null_values.remove(&t);
        if desc == NULL {
            b.null_labels.push(label);
            self.expect_null(b, expr);
        }
        b.labels.push((label, Dest::Jump(end), slots));
        self.expr_expect(b, expr, &desc)?;
        b.labels.pop();
        if desc == NULL {
            b.null_labels.pop();
        }
        b.code.place(end);
        b.code.set_stack(depth + slots);
        if adapt_join && b.code.live() {
            adapt_phantom(b, NULL);
        }
        Ok(slots)
    }

    /// `return[from] expr`: from a method or an enclosing labeled block; within a `finally` or
    /// `synchronized`, its value is stored and the cleanup runs first.
    pub fn gen_return(&mut self, b: &mut Body, t: TNodeId, expr: TNodeId, from: TNodeId) -> Result<i32> {
        let from = self.l.tpd.sym(from);
        if let Some(&(label, dest, slots)) = b.labels.iter().rev().find(|l| Some(l.0) == from) {
            if b.null_labels.contains(&label) {
                self.expect_null(b, expr);
            }
            self.expr_to(b, expr, slots, dest)?;
        } else if from.is_none() || from != b.method {
            return unsupported(self.l.tpd.span(t), "a return from an enclosing method");
        } else if let Some(&cleanup) = b.cleanups.last() {
            let result = b.result.clone();
            self.expr_as(b, expr, classfile::value_slots(&result))?;
            if result != "V" {
                let slot = b.early_return.unwrap_or_else(|| b.temp(&result));
                b.early_return = Some(slot);
                b.code.store(&result, slot);
            }
            b.code.jump(classfile::GOTO, cleanup);
            b.emit_cleanup = true;
        } else {
            let result = b.result.clone();
            self.expr_ret(b, expr, &result)?;
        }
        Ok(0)
    }
}
