// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! Conditionals and loops: then, else and merge blocks (nscplugin's `genIf`, also for link-time
//! conditions), condition, body and exit blocks (`genWhileDo`).

use super::Gen;
use crate::{Result, unsupported};
use nir::{Cond, Inst, Local, Next, TypeId, ValId};
use syntax::ast::Span;
use typer::tpd::TNodeId;

/// A part of a conditional: the span its instructions take, and what generates its value.
pub type Part<'g, 'l, 'a> = (Span, Box<dyn FnOnce(&mut Gen<'l, 'a>) -> Result<ValId> + 'g>);

/// What branches to the then and else blocks.
type Branch<'g, 'l, 'a> = Box<dyn FnOnce(&mut Gen<'l, 'a>, Local, Local) -> Result<()> + 'g>;

impl<'l, 'a> Gen<'l, 'a> {
    /// `if cond then thenp else elsep`, a `linktime if` for a link-time condition.
    pub fn gen_if(&mut self, retty: TypeId, cond: TNodeId, thenp: TNodeId, elsep: TNodeId) -> Result<ValId> {
        if let Some(c) = self.linktime_condition(cond)? {
            return self.gen_linktime_branches(retty, c, cond, [thenp, elsep]);
        }
        let part = |g: &Self, t: TNodeId| -> Part<'_, 'l, 'a> {
            (g.l.tpd.span(t), Box::new(move |g: &mut Gen<'l, 'a>| g.gen_expr(t)))
        };
        let parts = [part(self, cond), part(self, thenp), part(self, elsep)];
        self.gen_if_with(retty, parts)
    }

    /// `if cond then thenp else elsep` for a link-time condition `cond`.
    pub fn gen_linktime_if(
        &mut self,
        retty: TypeId,
        cond: TNodeId,
        thenp: TNodeId,
        elsep: TNodeId,
    ) -> Result<ValId> {
        let Some(c) = self.linktime_condition(cond)? else {
            return unsupported(self.l.tpd.span(cond), "a link-time property of a run-time condition");
        };
        self.gen_linktime_branches(retty, c, cond, [thenp, elsep])
    }

    fn gen_linktime_branches(
        &mut self,
        retty: TypeId,
        c: Cond,
        cond: TNodeId,
        parts: [TNodeId; 2],
    ) -> Result<ValId> {
        self.m.uses_linktime = true;
        let branch: Branch<'_, 'l, 'a> = Box::new(move |g, then_l, else_l| {
            let (then_l, else_l) = (Next::Label(then_l, Vec::new()), Next::Label(else_l, Vec::new()));
            g.emit_inst(Inst::LinktimeIf(Box::new(c), then_l, else_l));
            Ok(())
        });
        let [thenp, elsep] = parts.map(|t| -> Part<'_, 'l, 'a> {
            (self.l.tpd.span(t), Box::new(move |g: &mut Gen<'l, 'a>| g.gen_expr(t)))
        });
        self.gen_branches(retty, (self.l.tpd.span(cond), branch), [thenp, elsep])
    }

    /// `if cond then thenp else elsep`, each part at its span or the current position.
    pub fn gen_if_with(
        &mut self,
        retty: TypeId,
        [(span, cond), thenp, elsep]: [Part<'_, 'l, 'a>; 3],
    ) -> Result<ValId> {
        let branch: Branch<'_, 'l, 'a> = Box::new(move |g, then_l, else_l| {
            let c = cond(g)?;
            g.branch(c, then_l, else_l);
            Ok(())
        });
        self.gen_branches(retty, (span, branch), [thenp, elsep])
    }

    /// The then, else and merge blocks of a conditional branching by `branch` at its span.
    fn gen_branches(
        &mut self,
        retty: TypeId,
        (span, branch): (Span, Branch<'_, 'l, 'a>),
        [thenp, elsep]: [Part<'_, 'l, 'a>; 2],
    ) -> Result<ValId> {
        let (then_l, else_l, merge) = (self.fresh(), self.fresh(), self.fresh());
        let merge_v = self.fresh();
        let saved = self.at_span(span);
        branch(self, then_l, else_l)?;
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

    /// `while cond do body`: a condition block, a body block jumping back, an exit block.
    pub fn gen_while(&mut self, t: TNodeId, cond: TNodeId, body: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let (cond_l, body_l, exit_l) = (self.fresh(), self.fresh(), self.fresh());
        let saved = self.at_span(span);
        self.jump(cond_l, Vec::new());
        self.at_span(self.l.tpd.span(cond));
        self.label(cond_l, Vec::new());
        let endless = self.l.endless_loops.contains(&t);
        let c = if endless { self.val(nir::Val::True) } else { self.gen_expr(cond)? };
        self.branch(c, body_l, exit_l);
        self.at_span(self.l.tpd.span(body));
        self.label(body_l, Vec::new());
        self.gen_expr(body)?;
        self.jump(cond_l, Vec::new());
        self.m.pos = saved;
        self.at_span(span.end_pos());
        self.label(exit_l, Vec::new());
        self.m.pos = saved;
        if endless {
            let nothing = self.ty(nir::Type::Nothing);
            return Ok(self.val(nir::Val::Zero(nothing)));
        }
        Ok(self.val(nir::Val::Unit))
    }
}
