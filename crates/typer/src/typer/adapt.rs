//! Fitting a typed tree to what its context expects (dotty's `adapt`), as far as the subset
//! goes: a tree that fits as it is, a numeric constant converted, a value discarded for `Unit`.

use super::implicits::conversions::Goal;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{SymbolId, Type, TypeId};

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
        if let Some(Type::Method { params, result, implicit: false, .. }) = &method
            && params.is_empty()
            && self.is_auto_applicable(tree)
        {
            let apply = self.build().apply(tree, &[], *result, span.to_synthetic());
            return self.adapt(apply, pt, ctx);
        }
        if let Some(Type::Method { params, implicit: false, .. }) = &method
            && !params.is_empty()
            && !ctx.mode.is(Mode::PATTERN)
        {
            return match self.eta_expansion(tree, pt, ctx) {
                Ok(closure) => closure,
                Err(what) => {
                    self.unsupported_at(span, what);
                    tree
                }
            };
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
            return self.adapt(converted, Pt::Type(pt), ctx);
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

    /// Whether reference `tree` to a method with an empty parameter list may be called without
    /// one (dotty's auto-application, `matchNullaryLoosely`): a member of `Any` other than
    /// `getClass`, or of a `java.*` or `javax.*` class, or one overriding such a member.
    fn is_auto_applicable(&mut self, tree: TNodeId) -> bool {
        let Some(s) = self.tpd.sym(tree) else { return false };
        let Some(owner) = self.u.symbol(s).owner else { return false };
        if self.is_java_like_member(s) {
            return true;
        }
        let name = self.u.symbol(s).name.clone();
        let site = crate::defn::type_ref(self.u, owner);
        for base in self.u.linearization(owner).into_iter().skip(1) {
            for m in self.u.decl(base, &name).unwrap_or_default() {
                let private = self.u.symbol(m).flags.has(tasty::tags::PRIVATE);
                if !private && self.is_java_like_member(m) && self.same_member(site, m, s) {
                    return true;
                }
            }
        }
        false
    }

    /// Whether `s` is a member of `Any` other than `getClass`, or of a `java.*` or `javax.*` class.
    fn is_java_like_member(&mut self, s: SymbolId) -> bool {
        let Some(owner) = self.u.symbol(s).owner else { return false };
        if owner == self.u.builtins().any {
            return self.u.symbol(s).name != "getClass";
        }
        self.is_java_member(s)
    }

    /// `{ tree; () }` (dotty's `Block(tree :: Nil, unitLiteral)`).
    fn discarded(&mut self, tree: TNodeId, span: Span) -> TNodeId {
        let literal = self.build().literal(Constant::Unit, span.end_pos());
        let stats = self.tpd.list(&[tree]);
        self.build().block(stats, literal, span.to_synthetic())
    }

    /// The constant of `tree` converted to `pt` (dotty's `adaptToSubType` with `convertTo` and
    /// `adaptConstant`).
    fn converted_constant(&mut self, tree: TNodeId, pt: TypeId) -> Option<TNodeId> {
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
