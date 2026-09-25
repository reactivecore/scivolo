use super::Printer;
use crate::nir::{Cond, Inst, Next, Param};
use std::fmt::Write;

impl Printer<'_> {
    pub fn inst(&mut self, inst: &Inst) {
        match inst {
            Inst::Label(l, params) => {
                self.local(*l);
                if !params.is_empty() {
                    self.s("(");
                    self.sep(params, ", ", |p, param| p.param(param));
                    self.s(")");
                }
                self.s(":");
            }
            Inst::Let { id, op, unwind, scope } => {
                if *scope != 0 {
                    let _ = write!(self.out, "{scope}: ");
                }
                self.local(*id);
                self.s(" = ");
                self.op(op);
                self.unwind(unwind);
            }
            Inst::Ret(v) => self.op_args("ret ", &[*v]),
            Inst::Jump(n) => {
                self.s("jump ");
                self.next(n);
            }
            Inst::If(c, t, e) => {
                self.op_args("if ", &[*c]);
                self.branches(t, e);
            }
            Inst::LinktimeIf(cond, t, e) => {
                self.s("linktime if ");
                self.cond(cond);
                self.branches(t, e);
            }
            Inst::Switch(v, default, cases) => {
                self.op_args("switch ", &[*v]);
                self.s(" {");
                for c in cases {
                    self.s(" ");
                    self.next(c);
                }
                self.s(" default => ");
                self.next(default);
                self.s(" }");
            }
            Inst::Throw(v, unwind) => {
                self.op_args("throw ", &[*v]);
                self.unwind(unwind);
            }
            Inst::Unreachable(unwind) => {
                self.s("unreachable");
                self.unwind(unwind);
            }
        }
    }

    fn branches(&mut self, t: &Next, e: &Next) {
        self.s(" then ");
        self.next(t);
        self.s(" else ");
        self.next(e);
    }

    fn unwind(&mut self, n: &Next) {
        if !matches!(n, Next::None) {
            self.s(" ");
            self.next(n);
        }
    }

    fn cond(&mut self, c: &Cond) {
        match c {
            Cond::Simple { property, comp, value, .. } => {
                let _ =
                    write!(self.out, "{} {} ", self.pool.str(*property), format!("{comp:?}").to_lowercase());
                self.val(*value);
            }
            Cond::Complex { bin, left, right, .. } => {
                self.cond(left);
                let _ = write!(self.out, " {} ", format!("{bin:?}").to_lowercase());
                self.cond(right);
            }
        }
    }

    pub fn param(&mut self, p: &Param) {
        self.local(p.id);
        self.s(" : ");
        self.ty(p.ty);
    }
}
