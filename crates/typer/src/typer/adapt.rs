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
        let apply_selected = ctx.mode.is(Mode::APPLY_QUALIFIER);
        let ctx = ctx.with_mode(ctx.mode.without(Mode::APPLY_QUALIFIER));
        let Some(tpe) = self.tpd.get(tree).tpe else { return tree };
        if matches!(self.u.types().get(tpe), Type::Error) || matches!(pt, Pt::Fun(_) | Pt::PolyFun(_)) {
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
        if let Some(Type::Method { implicit: true, .. }) = &method {
            let info = self.method_info(tpe).unwrap_or(tpe);
            let meth = self.tpd.sym(self.tpd.meth_part(tree));
            let _ = self.constrain_applied_result(meth, info, pt);
            let applied =
                self.refinement_result(info, pt).and_then(|()| self.implicit_clause_applied(tree, ctx));
            return match applied {
                Ok(apply) => self.adapt(apply, pt, ctx),
                Err(what) => {
                    self.unsupported_at(span, what);
                    tree
                }
            };
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
        if !apply_selected && let Some(applied) = self.context_function_applied(tree, pt, ctx) {
            return applied.unwrap_or_else(|what| {
                self.unsupported_at(span, what);
                tree
            });
        }
        if let Some(inlined) = self.inlined_transparent(tree, pt, ctx) {
            return self.adapt(inlined, pt, ctx);
        }
        self.record_inline_call(tree, ctx);
        let Pt::Type(pt) = pt else { return tree };
        if ctx.mode.is(Mode::PATTERN) {
            if let Err(what) = self.check_pattern_equality(tree, pt, ctx) {
                self.unsupported_at(span, what);
            }
            return tree;
        }
        let lifts = self.u.opaque_lifts();
        if self.conforms(tpe, pt) {
            if self.u.opaque_lifts() != lifts && !self.conforms_unlifted(tpe, pt) {
                self.unsupported_at(span, "conformance through an opaque alias");
            }
            return tree;
        }
        if let Some(converted) = self.converted_constant(tree, pt) {
            return self.adapt(converted, Pt::Type(pt), ctx);
        }
        if let Some(widened) = self.numeric_view(tree, pt) {
            return widened;
        }
        if self.u.class_symbol(pt) == Some(self.defn.unit) {
            self.check_unit_purity(tree);
            return self.discarded(tree, span);
        }
        if let Some(applied) = self.case_companion_applied(tree, pt, ctx) {
            return applied.unwrap_or_else(|what| {
                self.unsupported_at(span, what);
                tree
            });
        }
        match self.converted(tree, Goal::Conform(pt), ctx) {
            Ok(Some(converted)) => converted,
            Ok(None) if self.defined_further(tpe, pt) => tree,
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

    /// The constant of `tree` converted to `pt` (dotty's `adaptToSubType` with `convertTo` and
    /// `adaptConstant`).
    pub fn converted_constant(&mut self, tree: TNodeId, pt: TypeId) -> Option<TNodeId> {
        let value = self.constant_of(tree)?;
        let converted = self.convert_constant(&value, pt).filter(|c| c.tag() != value.tag())?;
        let lossy = match (&value, &converted) {
            (Constant::Int(i), Constant::Float(f)) => *f as i32 != *i,
            (Constant::Long(l), Constant::Float(f)) => *f as i64 != *l,
            (Constant::Long(l), Constant::Double(d)) => *d as i64 != *l,
            _ => false,
        };
        if lossy {
            let short = converted.tag();
            let msg = format!(
                "Widening conversion from {} to {short} loses precision.\nWrite `.to{short}` instead.",
                value.tag()
            );
            self.diags.warning(self.tpd.span(tree).point(), msg);
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
