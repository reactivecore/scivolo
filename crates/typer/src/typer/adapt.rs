//! Fitting a typed tree to what its context expects (dotty's `adapt`), as far as the subset
//! goes: a tree that fits as it is, a numeric constant converted, a value discarded for `Unit`.

use super::implicits::conversions::Goal;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `tree` fitted to `pt`: unchanged if it conforms; a tree that needs more of dotty's `adapt`
    /// than the subset has is outside it.
    pub fn adapt(&mut self, tree: TNodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let Some(tpe) = self.tpd.get(tree).tpe else { return tree };
        if matches!(self.u.types().get(tpe), Type::Error) || matches!(pt, Pt::Fun(_) | Pt::PolyFun) {
            return tree;
        }
        let span = self.tpd.span(tree);
        let method = self.method_info(tpe).or(Some(tpe)).map(|i| self.u.types().get(i).clone());
        if let Some(Type::Poly { .. }) = method {
            return match self.with_type_vars(tree) {
                Ok(applied) => self.adapt(applied, pt, ctx),
                Err(what) => {
                    self.unsupported_at(span, what);
                    tree
                }
            };
        }
        if let Some(Type::Method { params, result, implicit: true, .. }) = &method {
            let mut args = Vec::new();
            for &(_, p) in params {
                match self.implicit_arg(p, span.end_pos(), ctx, 0) {
                    Ok(arg) => args.push(arg),
                    Err(what) => {
                        self.unsupported_at(span, what);
                        return tree;
                    }
                }
            }
            let apply = self.build().apply(tree, &args, *result, span.to_synthetic());
            return self.adapt(apply, pt, ctx);
        }
        if let Some(Type::Method { params, result, implicit: false, .. }) = &method {
            if let Some(arity) = self.eta_arity(tree, pt)
                && !self.is_constructor_ref(tree)
                && !ctx.mode.is(Mode::PATTERN)
            {
                return match self.eta_expansion(tree, pt, arity, ctx) {
                    Ok(closure) => closure,
                    Err(what) => {
                        self.unsupported_at(span, what);
                        tree
                    }
                };
            }
            if params.is_empty() && (self.is_constructor_ref(tree) || self.is_auto_applicable(tree)) {
                let apply = self.build().apply(tree, &[], *result, span.to_synthetic());
                self.fold_apply(apply);
                return self.adapt(apply, pt, ctx);
            }
        }
        if matches!(method, Some(Type::Method { .. } | Type::Poly { .. })) {
            self.unsupported_at(span, "method value");
            return tree;
        }
        let Pt::Type(pt) = pt else { return tree };
        if ctx.mode.is(Mode::PATTERN) {
            if let Err(what) = self.check_pattern_equality(tree, pt, ctx) {
                self.unsupported_at(span, what);
            }
            return tree;
        }
        if self.conforms(tpe, pt) {
            return tree;
        }
        if let Some(converted) = self.converted_constant(tree, pt) {
            return converted;
        }
        if let Some(widened) = self.numeric_view(tree, pt) {
            return widened;
        }
        if self.u.class_symbol(pt) == Some(self.defn.unit) {
            self.check_unit_purity(tree);
            return self.discarded(tree, span);
        }
        match self.converted(tree, Goal::Conform(pt), ctx) {
            Ok(Some(converted)) => converted,
            Ok(None) => self.type_mismatch(tree, pt, ctx).unwrap_or_else(|| {
                self.unsupported_at(span, "adaptation");
                tree
            }),
            Err(what) => {
                self.unsupported_at(span, what);
                tree
            }
        }
    }

    /// `{ tree; () }` (dotty's `Block(tree :: Nil, unitLiteral)`).
    fn discarded(&mut self, tree: TNodeId, span: Span) -> TNodeId {
        let literal = self.build().literal(Constant::Unit, span.end_pos());
        let stats = self.tpd.list(&[tree]);
        self.build().block(stats, literal, span.to_synthetic())
    }

    /// The constant of `tree` converted to the primitive class of `pt` (dotty's
    /// `Constant.convertTo`, `adaptConstant`).
    fn converted_constant(&mut self, tree: TNodeId, pt: TypeId) -> Option<TNodeId> {
        let value = self.constant_of(tree)?;
        let target = self.u.class_symbol(pt).map(|c| self.u.full_name(c))?;
        let (converted, lossy) = match (&value, target.as_str()) {
            (Constant::Char(c), "scala.Int") => (Constant::Int(*c as i32), false),
            (Constant::Int(i), "scala.Char") if (0..=0xFFFF).contains(i) => {
                (Constant::Char(*i as u16), false)
            }
            (Constant::Int(i), "scala.Long") => (Constant::Long(*i as i64), false),
            (Constant::Char(c), "scala.Long") => (Constant::Long(*c as i64), false),
            (Constant::Int(i), "scala.Float") => (Constant::Float(*i as f32), (*i as f32) as i32 != *i),
            (Constant::Long(l), "scala.Float") => (Constant::Float(*l as f32), (*l as f32) as i64 != *l),
            (Constant::Char(c), "scala.Float") => (Constant::Float(*c as f32), false),
            (Constant::Int(i), "scala.Double") => (Constant::Double(*i as f64), false),
            (Constant::Long(l), "scala.Double") => (Constant::Double(*l as f64), (*l as f64) as i64 != *l),
            (Constant::Char(c), "scala.Double") => (Constant::Double(*c as f64), false),
            (Constant::Float(f), "scala.Double") => (Constant::Double(*f as f64), false),
            _ => return None,
        };
        let span = self.tpd.span(tree);
        if lossy {
            let short = target.rsplit('.').next().unwrap_or(&target);
            let msg = format!(
                "Widening conversion from {} to {short} loses precision.\nWrite `.to{short}` instead.",
                value.tag()
            );
            self.diags.warning(span.point(), msg);
        }
        Some(self.with_constant(tree, converted))
    }

    /// `tree` evaluated for constant `value`: the literal alone if `tree` is pure.
    fn with_constant(&mut self, tree: TNodeId, value: Constant) -> TNodeId {
        let span = self.tpd.span(tree);
        let tpe = self.build().literal_type(&value);
        if let T::Block { stats, expr } = *self.tpd.tree(tree) {
            let expr = self.with_constant(expr, value);
            return self.build().block(stats, expr, span);
        }
        let pure = matches!(self.tpd.tree(tree), T::Literal { .. }) || self.is_idempotent_expr(tree);
        let literal = self.tpd.node(T::Literal { value }, span, Some(tpe), None);
        if pure {
            return literal;
        }
        let stats = self.tpd.list(&[tree]);
        self.build().block(stats, literal, span.to_synthetic())
    }
}
