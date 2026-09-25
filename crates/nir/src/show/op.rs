use super::Printer;
use crate::nir::{MemoryOrder, Op, TypeId, ValId};
use crate::pool::Pool;
use std::fmt::Write;

/// The text of `op` on its own.
pub fn text(pool: &Pool, op: &Op) -> String {
    let mut p = Printer::new(pool, true);
    p.op(op);
    p.out
}

impl Printer<'_> {
    fn typed(&mut self, name: &str, ty: TypeId) {
        self.s(name);
        self.s("[");
        self.ty(ty);
        self.s("] ");
    }

    pub fn op_args(&mut self, head: &str, vals: &[ValId]) {
        self.s(head);
        self.sep(vals, ", ", |p, v| p.val(*v));
    }

    fn order(&mut self, order: &Option<MemoryOrder>) {
        if let Some(o) = order {
            let _ = write!(self.out, " {}", format!("{o:?}").to_lowercase());
        }
    }

    pub fn op(&mut self, op: &Op) {
        match op {
            Op::Call { ty, ptr, args } => {
                self.typed("call", *ty);
                self.val(*ptr);
                self.s("(");
                self.sep(args, ", ", |p, v| p.val(*v));
                self.s(")");
            }
            Op::Load { ty, ptr, order } => {
                self.s(if order.is_some() { "atomic " } else { "" });
                self.typed("load", *ty);
                self.val(*ptr);
                self.order(order);
            }
            Op::Store { ty, ptr, value, order } => {
                self.s(if order.is_some() { "atomic " } else { "" });
                self.typed("store", *ty);
                self.op_args("", &[*ptr, *value]);
                self.order(order);
            }
            Op::Elem { ty, ptr, indexes } => {
                self.typed("elem", *ty);
                self.op_args("", &[*ptr]);
                self.s(", ");
                self.op_args("", indexes);
            }
            Op::Extract { aggr, indexes } | Op::Insert { aggr, indexes, .. } => {
                match op {
                    Op::Insert { value, .. } => self.op_args("insert ", &[*aggr, *value]),
                    _ => self.op_args("extract ", &[*aggr]),
                }
                let idx: Vec<String> = indexes.iter().map(i32::to_string).collect();
                let _ = write!(self.out, ", {}", idx.join(", "));
            }
            Op::Stackalloc { ty, n } => {
                self.typed("stackalloc", *ty);
                self.val(*n);
            }
            Op::Bin { bin, ty, l, r } => {
                self.typed(&format!("{bin:?}").to_lowercase(), *ty);
                self.op_args("", &[*l, *r]);
            }
            Op::Comp { comp, ty, l, r } => {
                self.typed(&format!("{comp:?}").to_lowercase(), *ty);
                self.op_args("", &[*l, *r]);
            }
            Op::Conv { conv, ty, value } => {
                self.typed(&format!("{conv:?}").to_lowercase(), *ty);
                self.val(*value);
            }
            Op::Fence(o) => self.s(&format!("fence {}", format!("{o:?}").to_lowercase())),
            Op::Classalloc { name, zone } => {
                self.s("classalloc ");
                self.global(*name);
                self.zone(zone);
            }
            Op::Fieldload { ty, obj, name } => {
                self.typed("fieldload", *ty);
                self.op_args("", &[*obj]);
                self.s(", ");
                self.global(*name);
            }
            Op::Fieldstore { ty, obj, name, value } => {
                self.typed("fieldstore", *ty);
                self.op_args("", &[*obj]);
                self.s(", ");
                self.global(*name);
                self.op_args(", ", &[*value]);
            }
            Op::Field { obj, name } => {
                self.op_args("field ", &[*obj]);
                self.s(", ");
                self.global(*name);
            }
            Op::Method { obj, sig } | Op::Dynmethod { obj, sig } => {
                let head = if matches!(op, Op::Method { .. }) { "method " } else { "dynmethod " };
                self.op_args(head, &[*obj]);
                self.s(", ");
                self.sig(self.pool.str(*sig));
            }
            Op::Module(g) => {
                self.s("module ");
                self.global(*g);
            }
            Op::As { ty, obj } => self.ty_val("as", *ty, *obj),
            Op::Is { ty, obj } => self.ty_val("is", *ty, *obj),
            Op::Box { ty, obj } => self.ty_val("box", *ty, *obj),
            Op::Unbox { ty, obj } => self.ty_val("unbox", *ty, *obj),
            Op::Copy(v) => self.op_args("copy ", &[*v]),
            Op::SizeOf(ty) => self.typed("sizeOf", *ty),
            Op::AlignmentOf(ty) => self.typed("alignmentOf", *ty),
            Op::Var(ty) => {
                self.typed("var", *ty);
                self.out.pop();
            }
            Op::Varload(v) => self.op_args("varload ", &[*v]),
            Op::Varstore { slot, value } => self.op_args("varstore ", &[*slot, *value]),
            Op::Arrayalloc { ty, init, zone } => {
                self.ty_val("arrayalloc", *ty, *init);
                self.zone(zone);
            }
            Op::Arrayload { ty, arr, idx } => {
                self.typed("arrayload", *ty);
                self.op_args("", &[*arr, *idx]);
            }
            Op::Arraystore { ty, arr, idx, value } => {
                self.typed("arraystore", *ty);
                self.op_args("", &[*arr, *idx, *value]);
            }
            Op::Arraylength(v) => self.op_args("arraylength ", &[*v]),
        }
    }

    fn ty_val(&mut self, name: &str, ty: TypeId, v: ValId) {
        self.typed(name, ty);
        self.val(v);
    }

    fn zone(&mut self, zone: &Option<ValId>) {
        if let Some(z) = zone {
            self.op_args(" inZone ", &[*z]);
        }
    }
}
