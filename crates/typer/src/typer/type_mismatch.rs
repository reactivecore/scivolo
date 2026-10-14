//! Trees whose type conforms neither to the expected type nor converts to it (dotty's
//! `TypeMismatch`), reported where dotty adapts no further and adds neither a note nor an addendum.

use super::Typer;
use super::constants::{is_numeric, of_type};
use super::implicits::conversions::Goal;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Type, TypeId};

/// The classes an expected type may not be for dotty to search a conversion.
const UNSPECIFIC: [&str; 5] =
    ["scala.Any", "scala.AnyVal", "scala.Matchable", "java.lang.Object", "scala.Unit"];

impl Typer<'_> {
    /// `TypeMismatch` for `tree` in `ctx`, whose type does not conform to `pt`, an error tree;
    /// `None` where dotty may adapt it otherwise, report it elsewhere or word the message otherwise.
    pub fn type_mismatch(&mut self, tree: TNodeId, pt: TypeId, ctx: Ctx) -> Option<TNodeId> {
        let span = self.written_span(tree);
        let found = self.tpd.tpe(tree);
        if span.is_synthetic() || !span.exists() || !self.is_plain_tree(tree) {
            return None;
        }
        let widened = self.u.widen(found);
        if self.has_vars(found) || self.has_vars(pt) || self.unspecific(widened, pt) {
            return None;
        }
        let (found_text, required) = (self.found_text(found)?, self.plain_text(pt)?);
        if !self.distinct_names(&[found, widened, pt])
            || self.inherits_twice(widened, pt)
            || self.constant_converts(tree, found, pt)
            || self.view_may_apply(widened, pt, ctx)
            || !self.certain_context(span, ctx)
        {
            return None;
        }
        let msg = super::errors::type_mismatch(&found_text, &required);
        self.diags.error_at(span.start(), span.end(), span.point(), msg);
        Some(self.error_tree(span))
    }

    /// The span of `tree`, as written for the constructor call `new C` stands for.
    fn written_span(&self, tree: TNodeId) -> Span {
        let span = self.tpd.span(tree);
        let T::Apply { fun, args } = *self.tpd.tree(tree) else { return span };
        match *self.tpd.tree(fun) {
            T::Select { qualifier, .. }
                if span.is_synthetic()
                    && self.tpd.items(args).is_empty()
                    && matches!(self.tpd.tree(qualifier), T::New { .. }) =>
            {
                Span::new(span.start(), span.end(), span.start())
            }
            _ => span,
        }
    }

    /// Whether a script base class of `widened` has two parents deriving from the class of `pt`,
    /// whose base type dotty may compute otherwise or reject.
    fn inherits_twice(&mut self, widened: TypeId, pt: TypeId) -> bool {
        let pt = self.u.dealias(pt);
        let (Some(from), Some(to)) = (self.u.class_symbol(widened), self.direct_class(pt)) else {
            return true;
        };
        for base in self.u.linearization(from) {
            if self.u.symbol(base).tree.is_some() {
                continue;
            }
            let parents = self.u.parents(base);
            let deriving = parents
                .into_iter()
                .filter(|&p| self.u.class_symbol(p).is_some_and(|c| self.u.derives_from(c, to)));
            if deriving.count() > 1 {
                return true;
            }
        }
        false
    }

    fn has_vars(&mut self, t: TypeId) -> bool {
        let parts = self.type_parts(t);
        parts
            .into_iter()
            .any(|p| matches!(self.u.types().get(p), Type::Var(_) | Type::Wildcard(_) | Type::Error))
    }

    /// Whether dotty searches no conversion from `widened` to `pt` (dotty's `isUnderspecified`,
    /// `TooUnspecific`), or the expected type has a special adaptation.
    fn unspecific(&mut self, widened: TypeId, pt: TypeId) -> bool {
        let b = self.u.builtins();
        let from = self.u.class_symbol(widened);
        if from.is_none_or(|c| c == b.null || c == b.nothing) {
            return true;
        }
        let pt = self.u.dealias(pt);
        let Some(to) = self.direct_class(pt) else { return true };
        let full = self.u.full_name(to);
        UNSPECIFIC.contains(&&*full) || full.starts_with("scala.ContextFunction")
    }

    /// Whether `found` is a numeric constant dotty converts to the primitive class of `pt` (dotty's
    /// `Constant.convertTo`), or `tree`, of a type other than a constant, denotes a constant.
    fn constant_converts(&mut self, tree: TNodeId, found: TypeId, pt: TypeId) -> bool {
        let Type::Constant(tag, bits) = *self.u.types().get(found) else {
            return !matches!(self.u.types().get(found), Type::Str(_)) && self.constant_of(tree).is_some();
        };
        let Some(c) = of_type(tag, bits).filter(is_numeric) else { return false };
        self.class_bound(pt).is_none() || self.convert_constant(&c, pt).is_some()
    }

    /// Whether an implicit in the unit, the implicit scope of `from` or `to`, or an object dotty's
    /// import suggestions search may convert `from` to `to`.
    fn view_may_apply(&mut self, from: TypeId, to: TypeId, ctx: Ctx) -> bool {
        let dealiased = self.u.dealias(from);
        let Some(q) = self.direct_class(dealiased) else { return true };
        let goal = Goal::Conform(to);
        let (Ok(mut scope), Ok(to_scope)) = (self.implicit_scope(from), self.implicit_scope(to)) else {
            return true;
        };
        scope.extend(to_scope);
        self.script_may_supply(None)
            || scope.into_iter().any(|c| self.may_view(c.sym, q, goal))
            || self.any_root_member(ctx, |t, d| t.may_view(d, q, goal))
    }
}
