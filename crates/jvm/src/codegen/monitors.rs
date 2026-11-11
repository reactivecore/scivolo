// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSyncAndTry.scala
//! `synchronized` between `monitorenter` and `monitorexit`, with a handler that exits the monitor
//! (dotty's `genSynchronized`).

use super::body::Body;
use super::tries::{nop_if_needed, pending_cleanups};
use super::{Gen, unsupported};
use classfile::value_slots;
use lower::Result;
use tasty::nme;
use typer::tpd::{List, TNodeId, Tree};

impl Gen<'_, '_> {
    /// Whether `fun` is `synchronized`, maybe applied to a type.
    pub fn is_synchronized(&self, fun: TNodeId) -> bool {
        let fun = match *self.l.tpd.tree(fun) {
            Tree::TypeApply { fun, .. } => fun,
            _ => fun,
        };
        let Some(s) = self.l.tpd.sym(fun) else { return false };
        let owner = self.l.u.symbol(s).owner;
        self.l.u.symbol(s).name == nme::SYNCHRONIZED
            && owner.is_some_and(|o| self.l.u.binary_name(o) == "java.lang.Object")
    }

    /// `receiver.synchronized(body)` of call `t` leaving a value of descriptor `result`, none if
    /// dropped.
    pub fn gen_synchronized(
        &mut self,
        b: &mut Body,
        t: TNodeId,
        (fun, args): (TNodeId, List),
        result: Option<String>,
    ) -> Result<i32> {
        let sel = match *self.l.tpd.tree(fun) {
            Tree::TypeApply { fun, .. } => fun,
            _ => fun,
        };
        let (&Tree::Select { qualifier, .. }, &[body]) = (self.l.tpd.tree(sel), self.l.tpd.items(args))
        else {
            return unsupported(self.l.tpd.span(fun), "`synchronized` without a receiver or body");
        };
        let monitor = b.temp("L");
        let cleanup = b.code.new_label();
        let result = result.filter(|r| r != "V");
        let slots = result.as_deref().map_or(0, value_slots);
        let kept = result.map(|r| (b.temp(&r), r));
        let depth = b.code.stack();
        self.expr_as(b, qualifier, 1)?;
        b.code.dup();
        b.code.store("L", monitor);
        b.code.insn(0xc2);
        let start = b.code.here();
        b.cleanups.push(cleanup);
        self.expr_as(b, body, slots)?;
        b.cleanups.pop();
        if let Some((slot, r)) = &kept {
            b.code.store(r, *slot);
        }
        nop_if_needed(b, start);
        let end = b.code.here();
        b.code.load("L", monitor);
        b.code.insn(0xc3);
        if let Some((slot, r)) = &kept {
            b.code.load(r, *slot);
        }
        let post = b.code.new_label();
        b.code.jump(0xa7, post);
        b.code.set_stack(depth + 1);
        let handler = b.code.here();
        b.code.protect(start, end, handler, 0);
        b.code.load("L", monitor);
        b.code.insn(0xc3);
        b.code.insn(0xbf);
        if b.emit_cleanup {
            b.code.place(cleanup);
            b.code.set_stack(depth);
            b.code.load("L", monitor);
            b.code.insn(0xc3);
            pending_cleanups(b);
        }
        b.code.place(post);
        b.code.set_stack(depth + slots);
        self.line(b, t);
        Ok(slots)
    }
}
