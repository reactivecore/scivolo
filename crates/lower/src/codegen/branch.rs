//! Conditionals: then, else and merge blocks (nscplugin's `genIf`).

use super::Gen;
use crate::Result;
use nir::{TypeId, ValId};
use typer::tpd::TNodeId;

type Part<'g, 'l, 'a> = Box<dyn FnOnce(&mut Gen<'l, 'a>) -> Result<ValId> + 'g>;

impl<'l, 'a> Gen<'l, 'a> {
    pub fn gen_if(&mut self, retty: TypeId, cond: TNodeId, thenp: TNodeId, elsep: TNodeId) -> Result<ValId> {
        let part = |t: TNodeId| -> Part<'_, 'l, 'a> {
            Box::new(move |g: &mut Gen<'l, 'a>| {
                let span = g.l.tpd.span(t);
                let saved = g.at_span(span);
                let v = g.gen_expr(t);
                g.m.pos = saved;
                v
            })
        };
        self.gen_if_with(retty, part(cond), part(thenp), part(elsep))
    }

    /// `if cond then thenp else elsep`, each part generating its value.
    pub fn gen_if_with(
        &mut self,
        retty: TypeId,
        cond: Part<'_, 'l, 'a>,
        thenp: Part<'_, 'l, 'a>,
        elsep: Part<'_, 'l, 'a>,
    ) -> Result<ValId> {
        let (then_l, else_l, merge) = (self.fresh(), self.fresh(), self.fresh());
        let merge_v = self.fresh();
        let c = cond(self)?;
        self.branch(c, then_l, else_l);
        for (label, part) in [(then_l, thenp), (else_l, elsep)] {
            self.label(label, Vec::new());
            let v = part(self)?;
            self.jump_exclude_unit(retty, merge, v);
        }
        Ok(self.label_exclude_unit(merge, merge_v, retty))
    }
}
