// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Branches, loops and destinations of values (dotty's `genLoadIfTo`, `genLoadTo`, `genWhileDo`).

use super::Gen;
use super::body::{Body, Dest};
use super::descriptors::{NOTHING, descriptor};
use lower::Result;
use syntax::ast::constant::Constant;
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// The descriptor of `t`'s erased type.
    pub fn desc_of(&mut self, t: TNodeId) -> String {
        descriptor(&self.l.u.erase(self.l.tpd.tpe(t)))
    }

    /// Generates `t` and returns its value of descriptor `result` from the method.
    pub fn expr_ret(&mut self, b: &mut Body, t: TNodeId, result: &str) -> Result<()> {
        self.expr_to(b, t, classfile::value_slots(result), Dest::Return)
    }

    /// Generates `t` leaving `slots` slots and jumps or returns with it to `dest` (dotty's `genLoadTo`);
    /// blocks, `if`s, labeled blocks and `match`es pass `dest` on to each branch instead.
    pub fn expr_to(&mut self, b: &mut Body, t: TNodeId, slots: i32, dest: Dest) -> Result<()> {
        if dest == Dest::Fall {
            return self.expr_as(b, t, slots);
        }
        match *self.l.tpd.tree(t) {
            Tree::Typed { expr, .. } => return self.expr_to(b, expr, slots, dest),
            Tree::Block { stats, expr } => {
                b.code.open_block();
                for s in self.l.tpd.items(stats).to_vec() {
                    self.block_stat(b, s)?;
                }
                self.expr_to(b, expr, slots, dest)?;
                b.code.close_block();
                return Ok(());
            }
            Tree::If { cond, thenp, elsep, .. } => {
                let (success, failure) = (b.code.new_label(), b.code.new_label());
                let depth = b.code.stack();
                self.cond(b, cond, success, failure, success)?;
                b.code.place(success);
                self.expr_to(b, thenp, slots, dest)?;
                b.code.place(failure);
                b.code.set_stack(depth);
                if !(self.is_missing_else(t, elsep) && slots == 0) {
                    return self.expr_to(b, elsep, slots, dest);
                }
                self.line(b, cond);
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
            Tree::Return { .. } => return self.expr(b, t).map(drop),
            Tree::Apply { fun, .. } if self.sends_phantom(b, t, dest) => {
                b.sent_phantom = Some(fun);
                self.expr_as(b, t, slots)?
            }
            _ if dest == Dest::Return => {
                let result = b.result.clone();
                self.expr_expect(b, t, &result)?
            }
            _ => self.expr_as(b, t, slots)?,
        }
        let call = matches!(self.l.tpd.tree(t), Tree::Apply { .. }) && !self.is_throw(t);
        if b.code.live() || call && self.desc_of(t) == NOTHING {
            match dest {
                Dest::Jump(l) => b.code.jump(classfile::GOTO, l),
                Dest::Return => {
                    let result = b.result.clone();
                    self.cast_to_null(b, t, &result);
                    b.code.ret(&result)
                }
                Dest::Throw(_) => b.code.insn(classfile::ATHROW),
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
        let desc = self.desc_of(t);
        let slots = classfile::value_slots(&desc);
        let (success, failure, after) = (b.code.new_label(), b.code.new_label(), b.code.new_label());
        let depth = b.code.stack();
        self.cond(b, cond, success, failure, success)?;
        b.code.place(success);
        if self.is_missing_else(t, elsep) && slots == 0 {
            self.expr_as(b, thenp, slots)?;
            b.code.place(failure);
            b.code.set_stack(depth);
            return Ok(0);
        }
        self.expr_to(b, thenp, slots, Dest::Jump(after))?;
        b.code.place(failure);
        b.code.set_stack(depth);
        self.expr_expect(b, elsep, &desc)?;
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
        self.expr_to(b, body, 0, Dest::Jump(top))?;
        b.code.place(exit);
        Ok(0)
    }

    /// Whether call `t` sent to `dest` passes on its `Nothing$` value as it is (dotty's `genLoadTo`
    /// not adapting a value of the expected type): thrown in a throw or returned from a method of
    /// type `Nothing`.
    fn sends_phantom(&self, b: &Body, t: TNodeId, dest: Dest) -> bool {
        let nothing = dest == Dest::Throw(true) || dest == Dest::Return && b.result == NOTHING;
        nothing && !self.is_throw(t)
    }

    /// Whether `elsep` of `if` `t` is a `()` added for a missing `else` (dotty's `SyntheticUnit`):
    /// of no extent or, compiling Scala's library, inlined from that library, at the call's span.
    fn is_missing_else(&self, t: TNodeId, elsep: TNodeId) -> bool {
        let at = self.l.tpd.span(elsep);
        let inlined = self.l.u.compiles_scala_library() && at == self.l.tpd.span(t);
        is_unit(self.l.tpd.tree(elsep)) && (!at.exists() || at.start() == at.end() || inlined)
    }
}

fn is_unit(t: &Tree) -> bool {
    matches!(t, Tree::Literal { value: Constant::Unit })
}
