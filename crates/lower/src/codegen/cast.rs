//! Conversions between types: casts `x.asInstanceOf[T]` between references (nscplugin's
//! `genAsInstanceOf`), and the `box` and `unbox` methods of the primitives' objects.

use super::Gen;
use crate::phases::adapt::erased_type;
use crate::phases::casts::conforms;
use crate::{Result, unsupported};
use nir::{Op, Type, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// A call of `box` or `unbox` method `sym` with `args`; `None` for another method.
    pub fn gen_boxing(&mut self, sym: SymbolId, args: &[TNodeId], span: Span) -> Result<Option<ValId>> {
        let unboxed = self.l.defs.unboxed_by(sym).map(|p| p.box_method);
        if !self.l.defs.is_box(sym) && unboxed.is_none() {
            return Ok(None);
        }
        let [arg] = args[..] else { unreachable!("one argument to `box` or `unbox`") };
        let value = self.gen_expr(arg)?;
        Ok(Some(match unboxed {
            Some(box_method) => {
                let ty = self.result_type(box_method);
                self.emit(Op::Unbox { ty, obj: value })
            }
            None => {
                let [primitive] = self.param_types(sym)[..] else { unreachable!("one parameter of `box`") };
                self.check_conforms(value, primitive, span)?;
                let ty = self.result_type(sym);
                self.emit(Op::Box { ty, obj: value })
            }
        }))
    }

    /// Type application `t` of `fun`: a cast of the selection's receiver to `t`'s type.
    pub fn gen_cast(&mut self, t: TNodeId, fun: TNodeId) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let Tree::Select { qualifier, .. } = *self.l.tpd.tree(fun) else {
            return unsupported(span, "type applications without a call");
        };
        if self.l.tpd.sym(fun) != Some(self.l.defs.as_instance_of) {
            return unsupported(span, "type applications without a call");
        }
        let (erased_from, erased_to) = (erased_type(self.l, qualifier), self.l.u.erase(self.l.tpd.tpe(t)));
        let subtype = conforms(self.l, &erased_from, &erased_to);
        let value = self.gen_expr(qualifier)?;
        let saved = self.at_span(span);
        let (from, to) = (self.val_ty(value), self.gen_type(self.l.tpd.tpe(t)));
        let reference = |t: &Type| matches!(t, Type::Ref { .. } | Type::Array { .. });
        let (f, ty) = (self.pool.ty(from).clone(), self.pool.ty(to).clone());
        let v = match () {
            _ if f == ty || matches!(f, Type::Null | Type::Nothing) || (subtype && reference(&f)) => value,
            _ if reference(&f) && reference(&ty) => self.emit(Op::As { ty: to, obj: value }),
            _ => return unsupported(span, "casts to or from primitive values"),
        };
        self.m.pos = saved;
        Ok(v)
    }
}
