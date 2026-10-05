//! Casts `x.asInstanceOf[T]` between references (nscplugin's `genAsInstanceOf`).

use super::Gen;
use crate::{Result, unsupported};
use nir::{Op, Type, ValId};
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// Type application `t` of `fun`: a cast of the selection's receiver to `t`'s type.
    pub fn gen_cast(&mut self, t: TNodeId, fun: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let Tree::Select { qualifier, .. } = *self.l.tpd.tree(fun) else {
            return unsupported(span, "type applications without a call");
        };
        if self.l.tpd.sym(fun) != Some(self.l.defs.as_instance_of) {
            return unsupported(span, "type applications without a call");
        }
        let value = self.gen_expr(qualifier)?;
        let saved = self.at_span(span);
        let (from, to) = (self.val_ty(value), self.gen_type(self.l.tpd.tpe(t)));
        let reference = |t: &Type| matches!(t, Type::Ref { .. } | Type::Array { .. });
        let (f, ty) = (self.pool.ty(from).clone(), self.pool.ty(to).clone());
        let v = match () {
            _ if f == ty || matches!(f, Type::Null | Type::Nothing) => value,
            _ if reference(&f) && reference(&ty) => self.emit(Op::As { ty: to, obj: value }),
            _ => return unsupported(span, "casts to or from primitive values"),
        };
        self.m.pos = saved;
        Ok(v)
    }
}
