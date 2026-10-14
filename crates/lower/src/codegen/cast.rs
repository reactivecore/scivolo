//! Conversions between types: casts `x.asInstanceOf[T]` between references (nscplugin's
//! `genAsInstanceOf`), the `box` and `unbox` methods of the primitives' objects, and the boxing
//! of values passed through erased signatures (`ensureBoxed`, `ensureUnboxed`).

use super::Gen;
use crate::phases::adapt::erased_type;
use crate::phases::casts::conforms;
use crate::symbols::is_singleton;
use crate::{Result, unsupported};
use nir::{Conv, Op, Type, TypeId, ValId};
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// `value` as type `to`: unboxed for a primitive, else cast (nscplugin's `ensureUnboxed`).
    pub fn ensure_unboxed(&mut self, value: ValId, to: TypeId) -> ValId {
        let from = self.val_ty(value);
        match self.boxed_class(to) {
            Some(_) if from == to => value,
            Some(boxed) => self.emit(Op::Unbox { ty: boxed, obj: value }),
            None => self.emit(Op::Conv { conv: Conv::Bitcast, ty: to, value }),
        }
    }

    /// `value` boxed if primitive and `ret` is not (nscplugin's `ensureBoxed`).
    pub fn ensure_boxed(&mut self, value: ValId, ret: TypeId) -> ValId {
        let from = self.val_ty(value);
        match self.boxed_class(from) {
            Some(boxed) if from != ret => self.emit(Op::Box { ty: boxed, obj: value }),
            _ => value,
        }
    }

    /// The class boxing values of primitive type `t`.
    fn boxed_class(&mut self, t: TypeId) -> Option<TypeId> {
        let letter = match self.pool.ty(t) {
            Type::Bool => 'z',
            Type::Char => 'c',
            Type::Byte => 'b',
            Type::Short => 's',
            Type::Int => 'i',
            Type::Long => 'j',
            Type::Float => 'f',
            Type::Double => 'd',
            _ => return None,
        };
        let box_method = self.l.defs.primitive(letter)?.box_method;
        Some(self.result_type(box_method))
    }

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
        if self.l.tpd.sym(fun) == Some(self.l.defs.patmat.is_instance_of) {
            let value = self.gen_expr(qualifier)?;
            let saved = self.at_span(span);
            let Tree::TypeApply { args, .. } = *self.l.tpd.tree(t) else {
                unreachable!("a type application")
            };
            let tested = self.l.u.erase_boxed(self.l.tpd.tpe(self.l.tpd.items(args)[0]));
            let ty = self.erased(&tested);
            let ty = self.bottom_box(ty).unwrap_or(ty);
            let v = self.emit(Op::Is { ty, obj: value });
            self.m.pos = saved;
            return Ok(v);
        }
        if self.l.tpd.sym(fun) != Some(self.l.defs.as_instance_of) {
            return unsupported(span, "type applications without a call");
        }
        let erased_from = erased_type(self.l, qualifier).full();
        let erased_to = self.l.u.erase_boxed(self.l.tpd.tpe(t));
        let subtype = conforms(self.l, &erased_from, &erased_to) && !self.l.explicit_casts.contains(&t);
        let value = self.gen_expr(qualifier)?;
        let saved = self.at_span(span);
        let (from, to) = (self.val_ty(value), self.erased(&erased_to));
        let reference = |t: &Type| matches!(t, Type::Ref { .. } | Type::Array { .. });
        let (f, ty) = (self.pool.ty(from).clone(), self.pool.ty(to).clone());
        let null_conforms = f == Type::Null
            && !matches!(ty, Type::Array { .. })
            && !is_singleton(self.l.u, self.l.tpd.tpe(t))
            && !self.l.null_casts.contains(&t);
        let same = match (&f, &ty) {
            (Type::Ref { name: a, .. }, Type::Ref { name: b, .. }) => a == b,
            (Type::Array { elem: a, .. }, Type::Array { elem: b, .. }) => a == b,
            _ => f == ty,
        };
        let v = match () {
            _ if same || f == Type::Nothing => value,
            _ if ty == Type::Nothing && (reference(&f) || f == Type::Null) => {
                self.gen_cast_to_nothing(value, from)
            }
            _ if null_conforms || (subtype && reference(&f)) => value,
            _ if (reference(&f) || f == Type::Null) && reference(&ty) => {
                self.emit(Op::As { ty: to, obj: value })
            }
            _ if ty == Type::Null && reference(&f) => {
                let ty = self.bottom_box(to).expect("`Null$`");
                self.emit(Op::As { ty, obj: value })
            }
            _ => return unsupported(span, "casts to or from primitive values"),
        };
        self.m.pos = saved;
        Ok(v)
    }

    /// `value` of type `from` cast to `Nothing`: a throw of `null` if it is null, else a cast to
    /// `Nothing$` that does not return (nscplugin's `genTypeApply`).
    fn gen_cast_to_nothing(&mut self, value: ValId, from: TypeId) -> ValId {
        let (is_null, not_null) = (self.fresh(), self.fresh());
        let null = self.val(nir::Val::Null);
        let cond = self.emit(Op::Comp { comp: nir::Comp::Ieq, ty: from, l: value, r: null });
        self.branch(cond, is_null, not_null);
        self.label(is_null, Vec::new());
        self.raise(null);
        self.label(not_null, Vec::new());
        let nothing = self.ty(Type::Nothing);
        let ty = self.bottom_box(nothing).expect("`Nothing$`");
        self.emit(Op::As { ty, obj: value });
        let unwind = self.unwind();
        self.emit_inst(nir::Inst::Unreachable(unwind));
        let dead = self.fresh();
        self.label(dead, Vec::new());
        self.val(nir::Val::Zero(nothing))
    }

    /// The class standing for the values of `Null` or `Nothing` type `t` (nscplugin's
    /// `genRefType`).
    fn bottom_box(&mut self, t: TypeId) -> Option<TypeId> {
        let name = match self.pool.ty(t) {
            Type::Null => "scala.runtime.Null$",
            Type::Nothing => "scala.runtime.Nothing$",
            _ => return None,
        };
        Some(self.class_type(name, false))
    }
}
