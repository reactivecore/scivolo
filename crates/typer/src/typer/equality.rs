//! Comparisons with `Any.==` and `Any.!=` (dotty's `checkCanEqual`), as far as the subset goes:
//! operands of which one conforms to the other, or two numbers, without `strictEquality`.

use super::Typer;
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use tasty::TypeId;

const NUMERIC: [&str; 7] =
    ["scala.Byte", "scala.Short", "scala.Char", "scala.Int", "scala.Long", "scala.Float", "scala.Double"];

impl Typer<'_> {
    /// Checks comparison `fun(arg)` of type `arg_tpe`, if `fun` selects `Any.==` or `Any.!=`.
    pub fn check_can_equal(&mut self, fun: TNodeId, arg_tpe: TypeId, ctx: Ctx) -> Result<(), &'static str> {
        let Some(sym) = self.tpd.sym(fun) else { return Ok(()) };
        let s = self.u.symbol(sym);
        if s.owner != Some(self.u.builtins().any) || !matches!(s.name.as_str(), "==" | "!=") {
            return Ok(());
        }
        let T::Select { qualifier, .. } = *self.tpd.tree(fun) else { return Err("comparison with this") };
        if self.strict_equality(ctx) {
            return Err("comparison under strictEquality");
        }
        let (a, b) = (self.tpd.tpe(qualifier), arg_tpe);
        let (a, b) = (self.u.widen(a), self.u.widen(b));
        let numeric = |t: &mut Self, x: TypeId| {
            t.u.class_symbol(x).is_some_and(|c| NUMERIC.contains(&t.u.full_name(c).as_str()))
        };
        let comparable =
            self.u.is_subtype(a, b) || self.u.is_subtype(b, a) || numeric(self, a) && numeric(self, b);
        if comparable { Ok(()) } else { Err("comparison of unrelated types") }
    }

    /// Whether `scala.language.strictEquality` is imported in `ctx`.
    fn strict_equality(&mut self, ctx: Ctx) -> bool {
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            if let Level::Import(i) = *self.levels.get(l)
                && !self.is_root_import(i)
                && let Some(info) = self.import_info(i)
            {
                let names: Vec<String> = info.selectors.iter().map(|s| s.name.clone()).collect();
                let qual = info.qual;
                let site = self.import_site(qual);
                let language = site.is_some_and(|s| self.u.full_name(s) == "scala.language");
                if language && names.iter().any(|n| n == "strictEquality" || n == "_") {
                    return true;
                }
            }
            level = self.levels.outer(l);
        }
        false
    }
}
