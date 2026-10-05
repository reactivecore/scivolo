//! Comparisons with `Any.==` and `Any.!=` (dotty's `checkCanEqual`), as far as the subset goes:
//! operands of which one conforms to the other, or two numbers, without `strictEquality`.

use super::Typer;
use super::widening::is_numeric;
use crate::tpd::{TNodeId, Tree as T};
use syntax::config::Features;
use tasty::TypeId;

impl Typer<'_> {
    /// Checks comparison `fun(arg)` of type `arg_tpe`, if `fun` selects `Any.==` or `Any.!=`.
    pub fn check_can_equal(&mut self, fun: TNodeId, arg_tpe: TypeId) -> Result<(), &'static str> {
        let Some(sym) = self.tpd.sym(fun) else { return Ok(()) };
        let s = self.u.symbol(sym);
        if s.owner != Some(self.u.builtins().any) || !matches!(s.name.as_str(), "==" | "!=") {
            return Ok(());
        }
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return Err("comparison with this") };
        if self.features.contains(Features::STRICT_EQUALITY) {
            return Err("comparison under strictEquality");
        }
        let a = self.tpd.tpe(qualifier);
        self.check_comparable(a, arg_tpe)
    }

    /// Checks that a value of type `pt` can be compared with literal or stable identifier
    /// pattern `pat` (dotty's `checkEqualityEvidence`).
    pub fn check_pattern_equality(&mut self, pat: TNodeId, pt: TypeId) -> Result<(), &'static str> {
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
        if self.features.contains(Features::STRICT_EQUALITY) {
            return Err("pattern under strictEquality");
        }
        self.check_comparable(tpe, pt)
    }

    fn check_comparable(&mut self, a: TypeId, b: TypeId) -> Result<(), &'static str> {
        let (a, b) = (self.zonk(a), self.zonk(b));
        let (a, b) = (self.u.widen(a), self.u.widen(b));
        let numeric = |t: &mut Self, x| t.value_class(x).is_some_and(|c| is_numeric(&c));
        let comparable =
            self.u.is_subtype(a, b) || self.u.is_subtype(b, a) || numeric(self, a) && numeric(self, b);
        if comparable { Ok(()) } else { Err("comparison of unrelated types") }
    }
}
