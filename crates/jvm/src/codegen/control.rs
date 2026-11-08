// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Branches, loops, labeled blocks and returns (dotty's `genLoadIfTo`, `genLoadTo`, `genWhileDo`,
//! `genLabeledTo`, `genReturn`).

use super::body::{Body, Dest};
use super::{Gen, descriptor, unsupported};
use lower::Result;
use syntax::ast::constant::Constant;
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// The descriptor of `t`'s erased type.
    pub fn desc_of(&mut self, t: TNodeId) -> String {
        let tpe = self.l.tpd.tpe(t);
        descriptor(&self.l.u.erase(tpe))
    }

    /// Generates `t` leaving `slots` slots: a value dropped, a `BoxedUnit` for none, as many as
    /// asked for a tree that does not fall through or is of type `Nothing`.
    pub fn expr_as(&mut self, b: &mut Body, t: TNodeId, slots: i32) -> Result<()> {
        let depth = b.code.stack();
        let got = self.expr(b, t)?;
        match got - slots {
            0 => {}
            _ if !b.code.live() => b.code.set_stack(depth + slots),
            1 if slots == 0 => b.code.insn(0x57),
            2 if slots == 0 => b.code.insn(0x58),
            -1 if got == 0 => {
                b.push("L");
                let unit = "scala/runtime/BoxedUnit";
                b.code.field(classfile::FieldOp::GetStatic, b.pool, unit, "UNIT", &format!("L{unit};"));
            }
            _ if got == 1 && self.desc_of(t) == "Lscala/runtime/Nothing$;" => {
                b.code.insn(0xbf);
                b.code.set_stack(depth + slots);
            }
            _ => return unsupported(self.l.tpd.span(t), "a value of another size than its context's"),
        }
        b.forget(depth + slots);
        Ok(())
    }

    /// Generates `t` and returns its value of descriptor `result` from the method.
    pub fn expr_ret(&mut self, b: &mut Body, t: TNodeId, result: &str) -> Result<()> {
        self.expr_to(b, t, classfile::value_slots(result), Dest::Return)
    }

    /// Generates `t` leaving `slots` slots and sends its value to `dest` (dotty's `genLoadTo`).
    pub fn expr_to(&mut self, b: &mut Body, t: TNodeId, slots: i32, dest: Dest) -> Result<()> {
        if dest == Dest::Fall {
            return self.expr_as(b, t, slots);
        }
        match *self.l.tpd.tree(t) {
            Tree::Typed { expr, .. } => return self.expr_to(b, expr, slots, dest),
            Tree::Block { stats, expr } => {
                for s in self.l.tpd.items(stats).to_vec() {
                    self.stat(b, s)?;
                }
                return self.expr_to(b, expr, slots, dest);
            }
            Tree::If { cond, thenp, elsep, .. } => {
                let (success, failure) = (b.code.new_label(), b.code.new_label());
                let depth = b.code.stack();
                self.cond(b, cond, success, failure, success)?;
                b.code.place(success);
                self.expr_to(b, thenp, slots, dest)?;
                b.code.place(failure);
                b.code.set_stack(depth);
                return self.expr_to(b, elsep, slots, dest);
            }
            Tree::Labeled { expr } => {
                let label = self.l.tpd.sym(t).expect("a label's symbol");
                b.labels.push((label, dest, slots));
                let done = self.expr_to(b, expr, slots, dest);
                b.labels.pop();
                return done;
            }
            Tree::Match { selector: Some(selector), cases, .. } => {
                self.gen_match(b, t, selector, cases, Some((slots, dest)))?;
                return Ok(());
            }
            Tree::Return { .. } => {
                self.expr(b, t)?;
                return Ok(());
            }
            _ => self.expr_as(b, t, slots)?,
        }
        if b.code.live() {
            match dest {
                Dest::Jump(l) => b.code.jump(0xa7, l),
                Dest::Return => {
                    let result = b.result.clone();
                    b.code.ret(&result)
                }
                Dest::Fall => {}
            }
        }
        Ok(())
    }

    /// `if` `t`; returns the slots of its value.
    pub fn gen_if(
        &mut self,
        b: &mut Body,
        t: TNodeId,
        cond: TNodeId,
        thenp: TNodeId,
        elsep: TNodeId,
    ) -> Result<i32> {
        let slots = classfile::value_slots(&self.desc_of(t));
        let (success, failure, after) = (b.code.new_label(), b.code.new_label(), b.code.new_label());
        let depth = b.code.stack();
        self.cond(b, cond, success, failure, success)?;
        b.code.place(success);
        if is_unit(self.l.tpd.tree(elsep)) && slots == 0 {
            self.expr_as(b, thenp, slots)?;
            b.code.place(failure);
            b.code.set_stack(depth);
            return Ok(0);
        }
        self.expr_to(b, thenp, slots, Dest::Jump(after))?;
        b.code.place(failure);
        b.code.set_stack(depth);
        self.expr_as(b, elsep, slots)?;
        b.code.place(after);
        b.code.set_stack(depth + slots);
        Ok(slots)
    }

    /// `while cond do body` `t`; an endless loop's labeled body jumps to the loop's start.
    pub fn gen_while(&mut self, b: &mut Body, t: TNodeId, cond: TNodeId, body: TNodeId) -> Result<i32> {
        let top = b.code.new_label();
        b.code.place(top);
        if self.l.is_endless_loop(t) {
            self.expr_to(b, body, 0, Dest::Jump(top))?;
            return Ok(0);
        }
        let (enter, exit) = (b.code.new_label(), b.code.new_label());
        if is_unit(self.l.tpd.tree(body)) {
            self.cond(b, cond, top, exit, exit)?;
            b.code.place(exit);
            return Ok(0);
        }
        self.cond(b, cond, enter, exit, enter)?;
        b.code.place(enter);
        self.expr_as(b, body, 0)?;
        b.code.jump(0xa7, top);
        b.code.place(exit);
        Ok(0)
    }

    /// Labeled block `t` whose value stays on the stack, which `return[t]` leaves.
    pub fn gen_labeled(&mut self, b: &mut Body, t: TNodeId, expr: TNodeId) -> Result<i32> {
        let label = self.l.tpd.sym(t).expect("a label's symbol");
        let slots = classfile::value_slots(&self.desc_of(t));
        let (end, depth) = (b.code.new_label(), b.code.stack());
        b.labels.push((label, Dest::Jump(end), slots));
        self.expr_as(b, expr, slots)?;
        b.labels.pop();
        b.code.place(end);
        b.code.set_stack(depth + slots);
        Ok(slots)
    }

    /// `return[from] expr`: from a method or an enclosing labeled block; within a `finally` or
    /// `synchronized`, its value is stored and the cleanup runs first.
    pub fn gen_return(&mut self, b: &mut Body, t: TNodeId, expr: TNodeId, from: TNodeId) -> Result<i32> {
        let from = self.l.tpd.sym(from);
        if let Some(&(_, dest, slots)) = b.labels.iter().rev().find(|l| Some(l.0) == from) {
            self.expr_to(b, expr, slots, dest)?;
        } else if from.is_none() || from != b.method {
            return unsupported(self.l.tpd.span(t), "a return from an enclosing method");
        } else if let Some(&cleanup) = b.cleanups.last() {
            let result = b.result.clone();
            self.expr_as(b, expr, classfile::value_slots(&result))?;
            if result != "V" {
                let slot = match b.early_return {
                    Some(slot) => slot,
                    None => b.temp(&result),
                };
                b.early_return = Some(slot);
                b.code.store(&result, slot);
            }
            b.code.jump(0xa7, cleanup);
            b.emit_cleanup = true;
        } else {
            let result = b.result.clone();
            self.expr_ret(b, expr, &result)?;
        }
        Ok(0)
    }
}

fn is_unit(t: &Tree) -> bool {
    matches!(t, Tree::Literal { value: Constant::Unit })
}
