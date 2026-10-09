//! `synchronized` (nscplugin's `genSynchronized`): the body between `enterMonitor` and
//! `exitMonitor` of the receiver, the exit on every edge leaving the body.

use super::Gen;
use crate::{Result, unsupported};
use nir::{Param, Type, Val, ValId};
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// A call of `synchronized` selected by `fun`, after evaluating the receiver if `fun` has a
    /// type argument (nscplugin's `genApplyTypeApply`).
    pub fn gen_synchronized(&mut self, fun: TNodeId, args: &[TNodeId]) -> Result<ValId> {
        let (sel, typed) = match *self.l.tpd.tree(fun) {
            Tree::TypeApply { fun, .. } => (fun, true),
            _ => (fun, false),
        };
        let (&Tree::Select { qualifier, .. }, &[body]) = (self.l.tpd.tree(sel), args) else {
            return unsupported(self.l.tpd.span(fun), "`synchronized` without a receiver or body");
        };
        if typed {
            self.gen_expr(qualifier)?;
        }
        self.gen_monitored(qualifier, body)
    }

    fn gen_monitored(&mut self, receiver: TNodeId, body: TNodeId) -> Result<ValId> {
        let monitors = self.l.defs.monitors;
        let span = self.l.tpd.span(receiver);
        let at = self.m.pos;
        let (normal, handler, merge) = (self.fresh(), self.fresh(), self.fresh());
        self.gen_apply_static(monitors.enter, None, &[receiver], span)?;
        let (result, insts) = self.nested(|g| -> Result<_> {
            let outer = g.m.unwind_handler.replace(handler);
            g.label(normal, Vec::new());
            let value = g.gen_expr(body);
            g.m.unwind_handler = outer;
            let value = value?;
            let nothing = g.gen_type(g.l.tpd.tpe(body));
            let retty = if *g.pool.ty(nothing) == Type::Nothing { nothing } else { g.val_ty(value) };
            let merge_v = g.fresh();
            g.jump_exclude_unit(retty, merge, value)?;
            let (exc, throwable) = (g.fresh(), g.throwable_type());
            g.label(handler, vec![Param { id: exc, ty: throwable }]);
            let exc = g.val(Val::Local(exc, throwable));
            g.raise(exc);
            let zero = g.val(Val::Zero(retty));
            g.jump_exclude_unit(retty, merge, zero)?;
            Ok((retty, merge_v))
        });
        let (retty, merge_v) = result?;
        let exit = |g: &mut Self| {
            let saved = g.at(at);
            g.gen_apply_static(monitors.exit, None, &[receiver], span)?;
            g.m.pos = saved;
            Ok(())
        };
        let insts = self.with_finally(exit, insts)?;
        self.jump(normal, Vec::new());
        self.append(insts);
        Ok(self.label_exclude_unit(merge, merge_v, retty))
    }
}
