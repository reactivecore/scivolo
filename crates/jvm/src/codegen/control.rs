// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Branches, loops, labeled blocks and returns (dotty's `genLoadIf`, `genLoadTo` with a return
//! or jump destination, `genWhileDo`, `genLabeled`, `genReturn`).

use super::method::Body;
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

    /// Generates `t` leaving `slots` slots: a value dropped, none for a tree that does not
    /// fall through.
    pub fn expr_as(&mut self, b: &mut Body, t: TNodeId, slots: i32) -> Result<()> {
        let got = self.expr(b, t)?;
        match got - slots {
            0 => {}
            d if !b.code.live() => b.code.set_stack(b.code.stack() - d),
            1 if slots == 0 => b.code.insn(0x57),
            2 if slots == 0 => b.code.insn(0x58),
            _ => return unsupported(self.l.tpd.span(t), "a value of another size than its context's"),
        }
        Ok(())
    }

    /// Generates `t` and returns its value of descriptor `result` from the method.
    pub fn expr_ret(&mut self, b: &mut Body, t: TNodeId, result: &str) -> Result<()> {
        match *self.l.tpd.tree(t) {
            Tree::Typed { expr, .. } => return self.expr_ret(b, expr, result),
            Tree::Block { stats, expr } => {
                for s in self.l.tpd.items(stats).to_vec() {
                    self.stat(b, s)?;
                }
                return self.expr_ret(b, expr, result);
            }
            Tree::If { cond, thenp, elsep, .. } if !is_unit(self.l.tpd.tree(elsep)) => {
                let (success, failure) = (b.code.new_label(), b.code.new_label());
                let depth = b.code.stack();
                self.cond(b, cond, success, failure, success)?;
                b.code.place(success);
                self.expr_ret(b, thenp, result)?;
                b.code.place(failure);
                b.code.set_stack(depth);
                return self.expr_ret(b, elsep, result);
            }
            _ => self.expr_as(b, t, classfile::value_slots(result))?,
        }
        if b.code.live() {
            b.code.ret(result);
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
        self.expr_as(b, thenp, slots)?;
        if is_unit(self.l.tpd.tree(elsep)) && slots == 0 {
            b.code.place(failure);
            return Ok(0);
        }
        if b.code.live() {
            b.code.jump(0xa7, after);
        }
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
            match (self.l.tpd.tree(body).clone(), self.l.tpd.sym(body)) {
                (Tree::Labeled { expr }, Some(label)) => {
                    b.labels.push((label, top, 0));
                    self.expr_as(b, expr, 0)?;
                    b.labels.pop();
                }
                _ => self.expr_as(b, body, 0)?,
            }
            if b.code.live() {
                b.code.jump(0xa7, top);
            }
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

    /// Labeled block `t`, which `return[t]` leaves.
    pub fn gen_labeled(&mut self, b: &mut Body, t: TNodeId, expr: TNodeId) -> Result<i32> {
        let label = self.l.tpd.sym(t).expect("a label's symbol");
        let slots = classfile::value_slots(&self.desc_of(t));
        let (end, depth) = (b.code.new_label(), b.code.stack());
        b.labels.push((label, end, slots));
        self.expr_as(b, expr, slots)?;
        b.labels.pop();
        b.code.place(end);
        b.code.set_stack(depth + slots);
        Ok(slots)
    }

    /// `return[from] expr`: from a method or an enclosing labeled block.
    pub fn gen_return(&mut self, b: &mut Body, t: TNodeId, expr: TNodeId, from: TNodeId) -> Result<i32> {
        let from = self.l.tpd.sym(from);
        if let Some(&(_, end, slots)) = b.labels.iter().rev().find(|l| Some(l.0) == from) {
            self.expr_as(b, expr, slots)?;
            b.code.jump(0xa7, end);
        } else if from.is_some() && from == b.method {
            let result = b.result.clone();
            self.expr_ret(b, expr, &result)?;
        } else {
            return unsupported(self.l.tpd.span(t), "a return from an enclosing method");
        }
        Ok(0)
    }
}

fn is_unit(t: &Tree) -> bool {
    matches!(t, Tree::Literal { value: Constant::Unit })
}
