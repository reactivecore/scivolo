//! Comparisons with `Any.==` and `Any.!=` (dotty's `checkCanEqual`), as far as the subset goes:
//! operands of which one conforms to the other, or two numbers, without `strictEquality`.

use super::Typer;
use super::widening::is_numeric;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::config::Features;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// Checks comparison `fun(arg)` of type `arg_tpe` in `ctx`, if `fun` selects `Any.==` or
    /// `Any.!=`.
    pub fn check_can_equal(&mut self, fun: TNodeId, arg_tpe: TypeId, ctx: Ctx) -> Result<(), &'static str> {
        let Some(sym) = self.tpd.sym(fun) else { return Ok(()) };
        let s = self.u.symbol(sym);
        if s.owner != Some(self.u.builtins().any) || !matches!(s.name.as_str(), "==" | "!=") {
            return Ok(());
        }
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return Err("comparison with this") };
        if self.feature_enabled(Features::STRICT_EQUALITY, "strictEquality", ctx) {
            return Err("comparison under strictEquality");
        }
        let a = self.tpd.tpe(qualifier);
        self.check_comparable(a, arg_tpe)
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
        if self.feature_enabled(Features::STRICT_EQUALITY, "strictEquality", ctx) {
            return Err("pattern under strictEquality");
        }
        self.check_comparable(tpe, pt)
    }

    /// Checks that values of types `a` and `b` can be compared: one conforms to the other lifted
    /// (dotty's `assumedCanEqual`), or both are numbers.
    fn check_comparable(&mut self, a: TypeId, b: TypeId) -> Result<(), &'static str> {
        let (a, b) = (self.fully_defined(a)?, self.fully_defined(b)?);
        let (a, b) = (self.u.widen(a), self.u.widen(b));
        let numeric = |t: &mut Self, x| t.value_class(x).is_some_and(|c| is_numeric(&c));
        let (lifted_a, lifted_b) = (self.lifted(a, 8), self.lifted(b, 8));
        let comparable = self.u.is_subtype(a, lifted_b)
            || self.u.is_subtype(b, lifted_a)
            || numeric(self, a) && numeric(self, b);
        if comparable { Ok(()) } else { Err("comparison of unrelated types") }
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
