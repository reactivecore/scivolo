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
        let (a, b) = (self.tpd.tpe(qualifier), arg_tpe);
        let (a, b) = (self.u.widen(a), self.u.widen(b));
        let numeric = |t: &mut Self, x| t.value_class(x).is_some_and(|c| is_numeric(&c));
        let comparable =
            self.u.is_subtype(a, b) || self.u.is_subtype(b, a) || numeric(self, a) && numeric(self, b);
        if comparable { Ok(()) } else { Err("comparison of unrelated types") }
    }
}
