//! Labeled blocks, returns from them, and matches on `Int`s and `String`s whose patterns are
//! literals (nscplugin's `genLabelDef`, `genReturn` and `genMatch`).

use super::Gen;
use crate::Result;
use nir::{Inst, Local, Next, Type, TypeId, Val, ValId};
use syntax::ast::Span;
use typer::tpd::{List, TNodeId, Tree};

/// A case of a switch: its block, value, body and span.
struct Case {
    id: Local,
    value: ValId,
    lit: TNodeId,
    body: TNodeId,
    span: Span,
}

impl Gen<'_, '_> {
    /// Labeled block `t`: an entry block, `expr`, an exit block taking the value.
    pub fn gen_labeled(&mut self, t: TNodeId, expr: TNodeId) -> Result<ValId> {
        let saved = self.at_span(self.l.tpd.span(t));
        let label = self.l.tpd.sym(t).expect("a label");
        let (entry, exit) = (self.fresh(), self.fresh());
        let param = self.fresh();
        let ty = self.gen_type(self.l.tpd.tpe(t));
        self.m.labels.insert(label, (exit, ty));
        self.jump(entry, Vec::new());
        self.label(entry, Vec::new());
        let v = self.gen_expr(expr)?;
        self.jump_exclude_unit(ty, exit, v)?;
        let out = self.label_exclude_unit(exit, param, ty);
        self.m.pos = saved;
        Ok(out)
    }

    /// `return[label] expr` of `t`, if `label` is an enclosing label: a jump to its exit.
    pub fn gen_label_return(&mut self, t: TNodeId, expr: TNodeId, from: TNodeId) -> Option<Result<ValId>> {
        let &(exit, ty) = self.m.labels.get(&self.l.tpd.sym(from)?)?;
        let saved = self.at_span(self.l.tpd.span(t));
        let result = self.gen_expr(expr).and_then(|v| self.jump_exclude_unit(ty, exit, v));
        self.m.pos = saved;
        Some(result.map(|()| self.val(Val::Unit)))
    }

    /// Match `t` on literals: a switch on an `Int`, else a chain of equality tests.
    pub fn gen_switch(&mut self, t: TNodeId, selector: TNodeId, cases: List) -> Result<ValId> {
        let saved = self.at_span(self.l.tpd.span(t));
        let mut switch_cases = Vec::new();
        let mut default = None;
        for &c in self.l.tpd.items(cases).to_vec().iter() {
            let Tree::CaseDef { pat, body, .. } = *self.l.tpd.tree(c) else { unreachable!("a case") };
            let lits = match *self.l.tpd.tree(pat) {
                Tree::Literal { .. } => vec![pat],
                Tree::Alternative { trees } => self.l.tpd.items(trees).to_vec(),
                _ => {
                    default = Some(body);
                    Vec::new()
                }
            };
            for lit in lits {
                let Tree::Literal { ref value } = *self.l.tpd.tree(lit) else { unreachable!("a literal") };
                let value = self.gen_literal(lit, &value.clone())?;
                let id = self.fresh();
                switch_cases.push(Case { id, value, lit, body, span: self.l.tpd.span(c) });
            }
        }
        let default = default.expect("a default case");
        let retty = self.gen_type(self.l.tpd.tpe(t));
        let scrut = self.gen_expr(selector)?;
        let int = self.ty(Type::Int);
        let ints = switch_cases.iter().all(|c| matches!(self.pool.val(c.value), Val::Int(_)));
        let out = if self.val_ty(scrut) == int && ints {
            self.gen_int_switch(retty, selector, default, &switch_cases)
        } else {
            self.gen_ifs_chain(retty, selector, default, &switch_cases)
        };
        self.m.pos = saved;
        out
    }

    fn gen_int_switch(
        &mut self,
        retty: TypeId,
        selector: TNodeId,
        default: TNodeId,
        cases: &[Case],
    ) -> Result<ValId> {
        let default_l = self.fresh();
        let (merge, merge_v) = (self.fresh(), self.fresh());
        let default_span = self.l.tpd.span(default);
        let scrut = self.gen_expr(selector)?;
        let nexts =
            cases.iter().map(|c| Next::Case(c.value, Box::new(Next::Label(c.id, Vec::new())))).collect();
        self.emit_inst(Inst::Switch(scrut, Next::Label(default_l, Vec::new()), nexts));
        let saved = self.at_span(default_span);
        self.label(default_l, Vec::new());
        let v = self.gen_expr(default)?;
        self.jump_exclude_unit(retty, merge, v)?;
        self.m.pos = saved;
        for c in cases {
            let saved = self.at_span(c.span);
            self.label(c.id, Vec::new());
            let v = self.gen_expr(c.body)?;
            self.jump_exclude_unit(retty, merge, v)?;
            self.m.pos = saved;
        }
        Ok(self.label_exclude_unit(merge, merge_v, retty))
    }

    fn gen_ifs_chain(
        &mut self,
        retty: TypeId,
        selector: TNodeId,
        default: TNodeId,
        cases: &[Case],
    ) -> Result<ValId> {
        let default_value = match *self.l.tpd.tree(default) {
            Tree::Labeled { expr } => Some(self.gen_labeled(default, expr)?),
            _ => None,
        };
        self.gen_cases(retty, selector, default, default_value, cases)
    }

    fn gen_cases(
        &mut self,
        retty: TypeId,
        selector: TNodeId,
        default: TNodeId,
        default_value: Option<ValId>,
        cases: &[Case],
    ) -> Result<ValId> {
        let Some((case, rest)) = cases.split_first() else {
            return match default_value {
                Some(v) => Ok(v),
                None => self.gen_expr(default),
            };
        };
        let saved = self.at_span(case.span);
        let span = self.l.tpd.span(case.body);
        let body = case.body;
        let cond = self.gen_class_equality(selector, case.lit, "==", case.span);
        let out = self.gen_if_with(
            retty,
            [
                (span, Box::new(move |_: &mut Self| cond)),
                (span, Box::new(move |g: &mut Self| g.gen_expr(body))),
                (
                    span,
                    Box::new(move |g: &mut Self| g.gen_cases(retty, selector, default, default_value, rest)),
                ),
            ],
        );
        self.m.pos = saved;
        out
    }
}
