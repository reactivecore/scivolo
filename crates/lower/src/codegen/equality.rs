// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/InterceptedMethods.scala
// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
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
use tasty::tags::FINAL;
use tasty::{Erased, SymbolId};
use typer::tpd::{TNodeId, Tree};

impl<'l, 'a> Gen<'l, 'a> {
    /// `left op right` on references for `op` one of `eq`, `ne`, `==` and `!=`, the last as
    /// `!(left == right)` with `==` at the start of `left` (dotty's `InterceptedMethods`).
    pub fn gen_class_equality(
        &mut self,
        left: TNodeId,
        right: TNodeId,
        op: &str,
        span: Span,
    ) -> Result<ValId> {
        if op == "!=" {
            let start = self.l.tpd.span(left);
            let inner = if start.exists() { Span::at(start.start()) } else { span };
            let saved = self.at_span(inner);
            let v = self.gen_class_equality(left, right, "==", inner)?;
            self.m.pos = saved;
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

    /// `if (left eq null) right eq null else left.equals(right)`, calling `Any.equals` as
    /// `any_equals`.
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

    /// `##` of `arg`: `0` for `null`, `Statics.longHash`, `doubleHash` or `floatHash` of a boxed
    /// `Long`, `Double` or `Float`, else `Statics.anyHash(arg)`.
    pub fn gen_hash_code(&mut self, arg: TNodeId, span: Span) -> Result<ValId> {
        if crate::phases::adapt::erased_type(self.l, arg) == Erased::Null {
            return Ok(self.val(Val::Int(0)));
        }
        let eq = self.l.equality()?;
        if let Some((letter, unboxed)) = self.boxed_primitive(arg) {
            let cc = self.l.case_classes()?;
            let hash = match letter {
                'j' => cc.long_hash,
                'd' => cc.double_hash,
                _ => cc.float_hash,
            };
            return self.gen_apply_static(hash, Some(eq.statics), &[unboxed], span);
        }
        self.gen_apply_static(eq.any_hash, Some(eq.statics), &[arg], span)
    }

    /// The letter of `Long`, `Double` or `Float` and the argument of `t` if `t` boxes a value
    /// whose type is that class, not an alias of it (dotty's `typeSymbol`).
    fn boxed_primitive(&mut self, t: TNodeId) -> Option<(char, TNodeId)> {
        let Tree::Apply { fun, args } = *self.l.tpd.tree(t) else { return None };
        let &[arg] = self.l.tpd.items(args) else { return None };
        let fun = self.l.tpd.sym(fun)?;
        let letter = "jdf".chars().find(|&c| self.l.primitive(c).is_some_and(|p| p.box_method == fun))?;
        let mut tpe = self.l.tpd.tpe(arg);
        while let Some(next) = self.l.u.widen_once(tpe) {
            tpe = next;
        }
        let class = self.l.u.class_symbol(tpe).filter(|_| self.l.u.dealias(tpe) == tpe)?;
        let name = ["scala.Long", "scala.Double", "scala.Float"]["jdf".find(letter)?];
        (self.l.u.full_name(class) == name).then_some((letter, arg))
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
            || self.l.tpd.sym(self.l.tpd.meth_part(t)).is_some_and(|s| self.l.u.is_module(s))
    }

    /// Whether either side may be a boxed number, character or boolean at run time, unless both
    /// are of the same final class.
    fn must_use_any_comparator(&mut self, left: TNodeId, right: TNodeId, eq: Equality) -> bool {
        let tpe = self.tree_type(left);
        let same_type = tpe == self.tree_type(right);
        let final_class = self.type_class(left).filter(|&c| has(self.l.u, c, FINAL));
        let same_finals =
            same_type && final_class.is_some_and(|c| tpe.1 == Erased::Class(self.l.u.binary_name(c)));
        !same_finals && self.is_maybe_boxed(left, eq) && self.is_maybe_boxed(right, eq)
    }

    /// Whether `t` erases to `Object`, `Serializable` or `Comparable`, or to a subclass of
    /// `Number`, `Character` or `Boolean`.
    fn is_maybe_boxed(&mut self, t: TNodeId, eq: Equality) -> bool {
        let Erased::Class(name) = crate::phases::adapt::erased_type(self.l, t) else { return false };
        if name == "java.lang.Object" {
            return true;
        }
        let c = match self.type_class(t) {
            Some(c) if self.l.u.binary_name(c) == name => c,
            _ => {
                let erased = self.l.u.class_ref(&name);
                let Some(c) = self.l.u.class_symbol(erased) else { return false };
                c
            }
        };
        let bases = [eq.number, eq.character, eq.boolean];
        [eq.serializable, eq.comparable].contains(&c)
            || bases.into_iter().any(|b| self.l.u.derives_from(c, b))
    }

    /// `t`'s type after dotty's erasure: its singleton type if it refers to `this`, a local value,
    /// an object or a field, else its erased type.
    fn tree_type(&mut self, t: TNodeId) -> (Option<tasty::TypeId>, Erased) {
        let tpe = self.l.tpd.tpe(t);
        let singleton = match *self.l.u.types().get(tpe) {
            tasty::Type::This(_) => true,
            tasty::Type::Sym(_, s) if !self.l.u.is_type_symbol(s) => {
                let in_class = self.l.u.symbol(s).owner.is_some_and(|o| self.l.u.is_class(o));
                !in_class || self.is_field(s) || self.l.u.is_module(s)
            }
            _ => false,
        };
        (singleton.then_some(tpe), crate::phases::adapt::erased_type(self.l, t))
    }

    fn type_class(&mut self, t: TNodeId) -> Option<SymbolId> {
        let tpe = self.l.u.widen(self.l.tpd.tpe(t));
        self.l.u.class_symbol(tpe)
    }
}
