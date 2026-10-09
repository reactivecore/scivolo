//! Trees whose type conforms neither to the expected type nor converts to it (dotty's
//! `TypeMismatch`), reported where dotty adapts no further and adds neither a note nor an addendum.

use super::Typer;
use super::implicits::conversions::Goal;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, Tree};
use tasty::tags::{ABSTRACT, INLINE, TRAIT};
use tasty::{Const, Kind, SymbolId, Type, TypeId, constant};

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
            || self.constant_converts(found, pt)
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

    /// Whether dotty types `tree` the same whatever the expected type: a literal, `this`, a
    /// reference to a member that is not overloaded, or an application of such a method that takes
    /// neither type parameters nor implicits.
    fn is_plain_tree(&mut self, tree: TNodeId) -> bool {
        match *self.tpd.tree(tree) {
            T::Literal { .. } | T::This { .. } => true,
            T::Ident { .. } | T::Select { .. } => {
                self.tpd.sym(tree).is_some_and(|s| self.is_single_member(tree, s))
            }
            T::Apply { fun, .. } => {
                let fun_tpe = self.tpd.tpe(fun);
                let info = self.method_info(fun_tpe).unwrap_or(fun_tpe);
                let explicit = matches!(self.u.types().get(info), Type::Method { implicit: false, .. });
                let new = match *self.tpd.tree(fun) {
                    T::Select { qualifier, .. } if matches!(self.tpd.tree(qualifier), T::New { .. }) => {
                        let class = self.tpd.tpe(qualifier);
                        Some(self.u.class_symbol(class).is_some_and(|c| self.is_concrete_class(c)))
                    }
                    _ => None,
                };
                let written = new.is_some() || !self.tpd.span(fun).is_synthetic();
                let plain_fun =
                    matches!(self.tpd.tree(fun), T::Ident { .. } | T::Select { .. } | T::Apply { .. });
                new != Some(false) && explicit && plain_fun && written && self.is_plain_tree(fun)
            }
            _ => false,
        }
    }

    /// Whether reference `tree` to `s`, not an inline method and of a settled type, finds no other
    /// member of that name in its prefix's class or else its owner, or `s` is local or top-level.
    fn is_single_member(&mut self, tree: TNodeId, s: SymbolId) -> bool {
        if self.u.symbol(s).flags.has(INLINE) || !self.type_is_settled(s) {
            return false;
        }
        let Some(owner) = self.u.symbol(s).owner else { return false };
        if self.u.symbol(owner).kind.is_term() || self.u.symbol(owner).kind == Kind::Package {
            return true;
        }
        let prefix = match *self.u.types().get(self.tpd.tpe(tree)) {
            Type::Sym(Some(p), _) if !matches!(self.u.types().get(p), Type::Package(_)) => Some(p),
            _ => None,
        };
        let class = match prefix {
            Some(p) => {
                let widened = self.u.widen(p);
                self.u.class_symbol(widened)
            }
            None => Some(owner).filter(|&o| self.u.symbol(o).kind == Kind::Class),
        };
        let Some(class) = class else { return false };
        let name = self.u.symbol(s).name.clone();
        self.class_members(class, &name).len() == 1
    }

    /// Whether the type of `s` does not rest on what the typer infers for its definition in this
    /// unit: a constructor's, written, or that of a literal right-hand side; not a closure's
    /// parameter's.
    pub fn type_is_settled(&self, s: SymbolId) -> bool {
        if self.u.symbol(s).name == "<init>" {
            return true;
        }
        if self.syms.closure_syms.contains(&s) {
            return false;
        }
        let Some((&id, _)) = self.syms.by_tree.iter().find(|&(_, &v)| v == s) else { return true };
        let (Tree::ValDef { tpt, rhs, .. } | Tree::DefDef { tpt, rhs, .. }) = *self.ast.tree(id) else {
            return true;
        };
        let literal =
            rhs.is_some_and(|r| matches!(self.ast.tree(r), Tree::Literal { .. } | Tree::Number { .. }));
        literal || !matches!(self.ast.tree(tpt), Tree::TypeTree) || self.splices.contains_key(&tpt)
    }

    /// Whether `new` may instantiate class `c`: neither abstract nor a trait.
    pub fn is_concrete_class(&self, c: SymbolId) -> bool {
        let flags = self.u.symbol(c).flags;
        !flags.has(ABSTRACT) && !flags.has(TRAIT)
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
    /// `Constant.convertTo`).
    fn constant_converts(&mut self, found: TypeId, pt: TypeId) -> bool {
        let Type::Constant(tag, bits) = *self.u.types().get(found) else { return false };
        let pt = self.u.dealias(pt);
        let Some(target) = self.direct_class(pt).map(|c| self.u.full_name(c)) else { return true };
        let (int, long) = match constant(tag, bits) {
            Some(Const::Byte(n)) => (Some(n as i64), Some(n as i64)),
            Some(Const::Short(n)) => (Some(n as i64), Some(n as i64)),
            Some(Const::Char(c)) => (Some(c as i64), Some(c as i64)),
            Some(Const::Int(n)) => (Some(n as i64), Some(n as i64)),
            Some(Const::Long(n)) => (None, Some(n)),
            Some(Const::Float(_)) => (None, None),
            Some(Const::Double(_)) => return target == "scala.Double",
            _ => return false,
        };
        match &*target {
            "scala.Byte" => int.is_some_and(|n| i8::try_from(n).is_ok()),
            "scala.Short" => int.is_some_and(|n| i16::try_from(n).is_ok()),
            "scala.Char" => int.is_some_and(|n| u16::try_from(n).is_ok()),
            "scala.Int" => int.is_some(),
            "scala.Long" => long.is_some(),
            "scala.Float" | "scala.Double" => true,
            _ => false,
        }
    }

    /// Whether an implicit in the unit, the implicit scope of `from` or `to`, or an object dotty's
    /// import suggestions search may convert `from` to `to`.
    fn view_may_apply(&mut self, from: TypeId, to: TypeId, ctx: Ctx) -> bool {
        let dealiased = self.u.dealias(from);
        let Some(q) = self.direct_class(dealiased) else { return true };
        let goal = Goal::Conform(to);
        let mut scope = self.implicit_scope(from);
        scope.extend(self.implicit_scope(to));
        self.script_may_supply(None)
            || scope.into_iter().any(|c| self.may_view(c.sym, q, goal))
            || self.any_root_member(ctx, |t, d| t.may_view(d, q, goal))
    }
}
