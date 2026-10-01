//! Conditionals: then, else and merge blocks (nscplugin's `genIf`).

use super::Gen;
use crate::Result;
use nir::{TypeId, ValId};
use syntax::ast::Span;
use typer::tpd::TNodeId;

/// A part of a conditional: the span its instructions take, and what generates its value.
pub type Part<'g, 'l, 'a> = (Span, Box<dyn FnOnce(&mut Gen<'l, 'a>) -> Result<ValId> + 'g>);

impl<'l, 'a> Gen<'l, 'a> {
    pub fn gen_if(&mut self, retty: TypeId, cond: TNodeId, thenp: TNodeId, elsep: TNodeId) -> Result<ValId> {
        let part = |g: &Self, t: TNodeId| -> Part<'_, 'l, 'a> {
            (g.l.tpd.span(t), Box::new(move |g: &mut Gen<'l, 'a>| g.gen_expr(t)))
        };
        let parts = [part(self, cond), part(self, thenp), part(self, elsep)];
        self.gen_if_with(retty, parts)
    }

    /// `if cond then thenp else elsep`, each part at its span or the current position.
    pub fn gen_if_with(
        &mut self,
        retty: TypeId,
        [cond, thenp, elsep]: [Part<'_, 'l, 'a>; 3],
    ) -> Result<ValId> {
        let (then_l, else_l, merge) = (self.fresh(), self.fresh(), self.fresh());
        let merge_v = self.fresh();
        let saved = self.at_span(cond.0);
        let c = (cond.1)(self)?;
        self.branch(c, then_l, else_l);
        self.m.pos = saved;
        for (label, (span, part)) in [(then_l, thenp), (else_l, elsep)] {
            let saved = self.at_span(span);
            self.label(label, Vec::new());
            let v = part(self)?;
            self.jump_exclude_unit(retty, merge, v)?;
            self.m.pos = saved;
        }
        Ok(self.label_exclude_unit(merge, merge_v, retty))
    }
}
