//! Equality and hashes of references: `eq`, `ne`, `==`, `!=`, `##` (nscplugin's
//! `genClassEquality`, `genClassUniversalEquality`, `genHashCode`).

use super::Gen;
use super::branch::Part;
use crate::Result;
use crate::definitions::Equality;
use crate::symbols::has;
use nir::{Bin, Comp, Op, Type, Val, ValId};
use std::cell::Cell;
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::SymbolId;
use tasty::tags::FINAL;
use typer::tpd::{TNodeId, Tree};

impl<'l, 'a> Gen<'l, 'a> {
    /// `left op right` on references for `op` one of `eq`, `ne`, `==` and `!=`, the last as
    /// `!(left == right)` (dotty's `InterceptedMethods`).
    pub fn gen_class_equality(
        &mut self,
        left: TNodeId,
        right: TNodeId,
        op: &str,
        span: Span,
    ) -> Result<ValId> {
        if op == "!=" {
            let v = self.gen_class_equality(left, right, "==", span)?;
            return Ok(self.negate(v));
        }
        if op != "==" || self.is_null(left) || self.is_null(right) {
            let comp = if op == "ne" { Comp::Ine } else { Comp::Ieq };
            let object = self.class_ref(self.l.defs.object);
            let (l, r) = (self.gen_expr(left)?, self.gen_expr(right)?);
            return Ok(self.emit(Op::Comp { comp, ty: object, l, r }));
        }
        let eq = self.l.equality()?;
        Ok(if self.must_use_any_comparator(left, right, eq) {
            self.gen_apply_static(eq.equals, Some(eq.boxes_runtime), &[left, right], span)?
        } else if self.is_non_null(left) {
            let l = self.gen_expr(left)?;
            let args = self.gen_args(eq.any_equals, &[right], span)?;
            self.call_method(eq.any_equals, false, l, args)
        } else {
            self.gen_null_checked_equals(left, right, eq.any_equals, span)?
        })
    }

    /// `if (left eq null) right eq null else left.equals(right)`, `any_equals` `Any.equals`.
    fn gen_null_checked_equals(
        &mut self,
        left: TNodeId,
        right: TNodeId,
        any_equals: SymbolId,
        span: Span,
    ) -> Result<ValId> {
        let object = self.class_ref(self.l.defs.object);
        let bool = self.ty(Type::Bool);
        let l = Cell::new(None);
        let l = &l;
        let parts: [Part<'_, 'l, 'a>; 3] = [
            (
                span,
                Box::new(move |g: &mut Gen<'l, 'a>| {
                    let v = g.gen_expr(left)?;
                    l.set(Some(v));
                    let null = g.val(Val::Null);
                    Ok(g.emit(Op::Comp { comp: Comp::Ieq, ty: object, l: v, r: null }))
                }),
            ),
            (
                span,
                Box::new(move |g: &mut Gen<'l, 'a>| {
                    let r = g.gen_expr(right)?;
                    let null = g.val(Val::Null);
                    Ok(g.emit(Op::Comp { comp: Comp::Ieq, ty: object, l: r, r: null }))
                }),
            ),
            (
                span,
                Box::new(move |g: &mut Gen<'l, 'a>| {
                    let args = g.gen_args(any_equals, &[right], span)?;
                    let receiver = l.get().expect("the left operand");
                    Ok(g.call_method(any_equals, false, receiver, args))
                }),
            ),
        ];
        self.gen_if_with(bool, parts)
    }

    /// `##` of `arg`: `Statics.anyHash(arg)`.
    pub fn gen_hash_code(&mut self, arg: TNodeId, span: Span) -> Result<ValId> {
        let eq = self.l.equality()?;
        self.gen_apply_static(eq.any_hash, Some(eq.statics), &[arg], span)
    }

    fn negate(&mut self, v: ValId) -> ValId {
        let (bool, tru) = (self.ty(Type::Bool), self.val(Val::True));
        self.emit(Op::Bin { bin: Bin::Xor, ty: bool, l: tru, r: v })
    }

    fn is_null(&self, t: TNodeId) -> bool {
        matches!(self.l.tpd.tree(t), Tree::Literal { value: Constant::Null })
    }

    fn is_non_null(&self, t: TNodeId) -> bool {
        matches!(self.l.tpd.tree(t), Tree::Literal { .. } | Tree::ClassLiteral { .. })
            || self.l.tpd.sym(t).is_some_and(|s| self.l.u.is_module(s))
    }

    /// Whether either side may be a boxed number, character or boolean at run time, unless both
    /// are of the same final class.
    fn must_use_any_comparator(&mut self, left: TNodeId, right: TNodeId, eq: Equality) -> bool {
        let same_type = self.l.tpd.tpe(left) == self.l.tpd.tpe(right);
        let same_finals = same_type && self.type_class(left).is_some_and(|c| has(self.l.u, c, FINAL));
        !same_finals && self.is_maybe_boxed(left, eq) && self.is_maybe_boxed(right, eq)
    }

    /// Whether `t` erases to `Object`, `Serializable` or `Comparable`, or to a subclass of
    /// `Number`, `Character` or `Boolean`.
    fn is_maybe_boxed(&mut self, t: TNodeId, eq: Equality) -> bool {
        let erased = self.gen_type(self.l.tpd.tpe(t));
        if erased == self.class_ref(self.l.defs.object) {
            return true;
        }
        let Some(c) = self.type_class(t) else { return false };
        let bases = [eq.number, eq.character, eq.boolean];
        [eq.serializable, eq.comparable].contains(&c)
            || bases.into_iter().any(|b| self.l.u.derives_from(c, b))
    }

    fn type_class(&mut self, t: TNodeId) -> Option<SymbolId> {
        let tpe = self.l.u.widen(self.l.tpd.tpe(t));
        self.l.u.class_symbol(tpe)
    }
}
