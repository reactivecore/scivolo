use super::{Out, Writer};
use crate::nir::{ORDERS, Op};
use crate::tags::op;

impl Writer<'_> {
    /// The operations on objects, arrays, variables and the memory model.
    pub fn object_op(&mut self, out: &mut Out, o: &Op) {
        match o {
            Op::Fence(order) => {
                out.u8(op::FENCE);
                out.u8(ORDERS.iter().position(|t| t == order).expect("an order") as u8 + 1);
            }
            Op::Classalloc { name, zone } => {
                out.u8(if zone.is_some() { op::CLASSALLOC_ZONE } else { op::CLASSALLOC });
                self.global(out, *name);
                if let Some(z) = zone {
                    self.val(out, *z);
                }
            }
            Op::Fieldload { ty, obj, name } => {
                out.u8(op::FIELDLOAD);
                self.ty(out, *ty);
                self.val(out, *obj);
                self.global(out, *name);
            }
            Op::Fieldstore { ty, obj, name, value } => {
                out.u8(op::FIELDSTORE);
                self.ty(out, *ty);
                self.val(out, *obj);
                self.global(out, *name);
                self.val(out, *value);
            }
            Op::Field { obj, name } => {
                out.u8(op::FIELD);
                self.val(out, *obj);
                self.global(out, *name);
            }
            Op::Method { obj, sig } | Op::Dynmethod { obj, sig } => {
                out.u8(if matches!(o, Op::Method { .. }) { op::METHOD } else { op::DYNMETHOD });
                self.val(out, *obj);
                self.string(out, *sig);
            }
            Op::Module(g) => {
                out.u8(op::MODULE);
                self.global(out, *g);
            }
            Op::As { ty, obj } | Op::Is { ty, obj } | Op::Box { ty, obj } | Op::Unbox { ty, obj } => {
                out.u8(match o {
                    Op::As { .. } => op::AS,
                    Op::Is { .. } => op::IS,
                    Op::Box { .. } => op::BOX,
                    _ => op::UNBOX,
                });
                self.ty(out, *ty);
                self.val(out, *obj);
            }
            Op::Copy(v) | Op::Varload(v) | Op::Arraylength(v) => {
                out.u8(match o {
                    Op::Copy(_) => op::COPY,
                    Op::Varload(_) => op::VARLOAD,
                    _ => op::ARRAYLENGTH,
                });
                self.val(out, *v);
            }
            Op::SizeOf(t) | Op::AlignmentOf(t) | Op::Var(t) => {
                out.u8(match o {
                    Op::SizeOf(_) => op::SIZE_OF,
                    Op::AlignmentOf(_) => op::ALIGNMENT_OF,
                    _ => op::VAR,
                });
                self.ty(out, *t);
            }
            Op::Varstore { slot, value } => {
                out.u8(op::VARSTORE);
                self.val(out, *slot);
                self.val(out, *value);
            }
            Op::Arrayalloc { ty, init, zone } => {
                out.u8(if zone.is_some() { op::ARRAYALLOC_ZONE } else { op::ARRAYALLOC });
                self.ty(out, *ty);
                self.val(out, *init);
                if let Some(z) = zone {
                    self.val(out, *z);
                }
            }
            Op::Arrayload { ty, arr, idx } => {
                out.u8(op::ARRAYLOAD);
                self.ty(out, *ty);
                self.val(out, *arr);
                self.val(out, *idx);
            }
            Op::Arraystore { ty, arr, idx, value } => {
                out.u8(op::ARRAYSTORE);
                self.ty(out, *ty);
                self.val(out, *arr);
                self.val(out, *idx);
                self.val(out, *value);
            }
            _ => unreachable!("written by `op`"),
        }
    }
}
