//! Comparisons with `Any.==` and `Any.!=` (dotty's `checkCanEqual`): operands of which one
//! conforms to the other, else a `CanEqual` instance found or synthesized.

use super::Typer;
use super::implicits::is_misfit;
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Ast, NodeId, Span, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Checks comparison `fun(arg)` at `at` of type `arg_tpe` in `ctx`, if `fun` selects `Any.==`
    /// or `Any.!=`; an error reported there if no `CanEqual` instance exists.
    pub fn check_can_equal(
        &mut self,
        fun: TNodeId,
        arg_tpe: TypeId,
        at: Span,
        ctx: Ctx,
    ) -> Result<(), &'static str> {
        let Some(sym) = self.tpd.sym(fun) else { return Ok(()) };
        let s = self.u.symbol(sym);
        if s.owner != Some(self.u.builtins().any) || !matches!(s.name.as_str(), "==" | "!=") {
            return Ok(());
        }
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return Err("comparison with this") };
        let view = self.tpd.sym(self.tpd.meth_part(qualifier)).is_some_and(|s| {
            let flags = self.u.symbol(s).flags;
            flags.has(tasty::tags::IMPLICIT) || flags.has(tasty::tags::GIVEN)
        });
        if matches!(*self.tpd.tree(qualifier), T::Apply { args, .. } if !self.tpd.items(args).is_empty())
            && self.tpd.span(qualifier).is_synthetic()
            && view
        {
            return Err("comparison of a converted value");
        }
        let a = self.tpd.tpe(qualifier);
        self.check_comparable(a, arg_tpe, at, true, ctx)
    }

    /// Checks that a value of type `pt` can be compared with literal or stable identifier
    /// pattern `pat` in `ctx` (dotty's `checkEqualityEvidence`).
    pub fn check_pattern_equality(&mut self, pat: TNodeId, pt: TypeId, ctx: Ctx) -> Result<(), &'static str> {
        let wildcard = matches!(*self.tpd.tree(pat), T::Ident { name } if self.text(name.name) == "_");
        if !matches!(self.tpd.tree(pat), T::Ident { .. } | T::Select { .. } | T::Literal { .. }) || wildcard {
            return Ok(());
        }
        let path = matches!(self.tpd.tree(pat), T::Ident { .. } | T::Select { .. });
        let package = self.tpd.sym(pat).is_some_and(|s| self.u.symbol(s).kind == tasty::Kind::Package);
        if path && (package || !self.is_stable_path(pat)) {
            return Err("pattern that is not a stable identifier");
        }
        let tpe = self.tpd.tpe(pat);
        if self.u.is_subtype(pt, tpe) {
            return Ok(());
        }
        let span = self.tpd.span(pat);
        self.check_comparable(tpe, pt, span, false, ctx)
    }

    /// Checks that values of types `a` and `b` can be compared at `at` in `ctx`: one conforms to
    /// the other lifted, else a `CanEqual[a, b]` instance exists; if none does, an error reported
    /// there if `report` and dotty's message is certain, else `Err`.
    fn check_comparable(
        &mut self,
        a: TypeId,
        b: TypeId,
        at: Span,
        report: bool,
        ctx: Ctx,
    ) -> Result<(), &'static str> {
        let (a, b) = (self.fully_defined(a)?, self.fully_defined(b)?);
        let (a, b) = (self.u.widen(a), self.u.widen(b));
        let ctx = ctx.with_mode(Mode::NONE);
        if !self.is_strict_equality(ctx) && self.assumed_can_equal(a, b) {
            return Ok(());
        }
        let pt = self.can_equal_type(a, b).ok_or("CanEqual")?;
        match self.implicit_arg(pt, at, ctx, 0) {
            Ok(_) => Ok(()),
            Err(what) if !is_misfit(what) => Err(what),
            Err(_) => match (self.plain_text(a), self.plain_text(b)) {
                (Some(l), Some(r)) if report && self.certainly_incomparable(a, b, at, ctx) => {
                    self.diags.error_at(
                        at.start(),
                        at.end(),
                        at.point(),
                        super::errors::cannot_compare(&l, &r),
                    );
                    Ok(())
                }
                _ => Err("comparison of unrelated types"),
            },
        }
    }

    /// Whether dotty finds no `CanEqual[a, b]` at `at` in `ctx` either, as far as the search can
    /// tell: outside `strictEquality`, the unit defines no implicit and derives nothing, and every
    /// class `a` and `b` refer to is no object's and has all its base classes in the library.
    fn certainly_incomparable(&mut self, a: TypeId, b: TypeId, at: Span, ctx: Ctx) -> bool {
        let derives = |ast: &Ast, n: NodeId| matches!(*ast.tree(n), Tree::Template { derived: Some(_), .. });
        if self.is_strict_equality(ctx)
            || self.script_may_supply(None)
            || self.ast.exists_subtree(self.unit, &derives)
        {
            return false;
        }
        let parts = [self.type_parts(a), self.type_parts(b)].concat();
        let classes: Vec<_> = parts.into_iter().filter_map(|p| self.u.class_symbol(p)).collect();
        classes.into_iter().all(|c| !self.u.is_module_class(c) && self.has_known_bases(c))
            && self.certain_context(at, ctx)
    }

    /// Whether every parent of class `c` and of its base classes resolves to a class.
    fn has_known_bases(&mut self, c: SymbolId) -> bool {
        self.u.linearization(c).into_iter().all(|base| {
            let parents = self.u.parents(base);
            parents.into_iter().all(|p| self.u.class_symbol(p).is_some())
        })
    }

    /// Whether one of `a` and `b` conforms to the other lifted (dotty's `assumedCanEqual`).
    pub fn assumed_can_equal(&mut self, a: TypeId, b: TypeId) -> bool {
        let (lifted_a, lifted_b) = (self.lifted(a, 8), self.lifted(b, 8));
        self.u.is_subtype(a, lifted_b) || self.u.is_subtype(b, lifted_a)
    }

    /// `t` with abstract types replaced by their upper bounds, singletons widened and refinements
    /// dropped, in covariant positions (dotty's `lift` in `assumedCanEqual`).
    fn lifted(&mut self, t: TypeId, depth: u32) -> TypeId {
        let Some(depth) = depth.checked_sub(1) else { return t };
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) if matches!(self.u.symbol(s).kind, Kind::Type | Kind::TypeParam) => {
                self.complete(s);
                let info = self.u.info(s);
                match *self.u.types().get(info) {
                    Type::Bounds(lo, hi) if lo != hi && !self.u.symbol(s).flags.has(tasty::tags::OPAQUE) => {
                        self.lifted(hi, depth)
                    }
                    _ => t,
                }
            }
            Type::Sym(..) | Type::TermRef(..) | Type::Constant(..) | Type::Str(_) | Type::This(_)
                if self.u.widen(t) != t =>
            {
                let widened = self.u.widen(t);
                self.lifted(widened, depth)
            }
            Type::Refined(parent, ..) => self.lifted(parent, depth),
            Type::Applied(tycon, args) => {
                let variances = self.u.class_symbol(tycon).map(|c| self.u.variances(c)).unwrap_or_default();
                let args = args
                    .iter()
                    .enumerate()
                    .map(|(i, &a)| if variances.get(i) == Some(&1) { self.lifted(a, depth) } else { a })
                    .collect();
                self.u.intern_type(Type::Applied(tycon, args))
            }
            _ => t,
        }
    }
}
