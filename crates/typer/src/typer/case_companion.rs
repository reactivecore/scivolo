// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! A case class companion adapted to a function type by inserting `apply` (dotty's
//! `adaptNoArgsOther`'s `caseCompanion`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `tree.apply` fitted to function type `pt` for a reference `tree` to the companion of a case
    /// class, with dotty's warning.
    pub fn case_companion_applied(
        &mut self,
        tree: TNodeId,
        pt: TypeId,
        ctx: Ctx,
    ) -> Option<Result<TNodeId, &'static str>> {
        let T::Ident { name } = *self.tpd.tree(tree) else { return None };
        let module = self.tpd.sym(tree).filter(|&m| self.u.is_module(m))?;
        let module_class = self.u.module_class(module)?;
        let class = self.u.companion_class(module_class)?;
        let is_function = |c: &str| {
            let arity = c.strip_prefix("scala.Function").or_else(|| c.strip_prefix("scala.ContextFunction"));
            arity.is_some_and(|n| n.parse::<u8>().is_ok())
        };
        if !self.u.symbol(class).flags.has(tasty::tags::CASE)
            || !(0..=22).any(|n| self.function_parts(pt, n).is_some())
            || self.u.linearization(module_class).into_iter().any(|c| is_function(&self.u.full_name(c)))
        {
            return None;
        }
        let prefix = match *self.u.types().get(self.tpd.tpe(tree)) {
            Type::Sym(Some(p), _) => self.shown_prefix(p),
            _ => String::new(),
        };
        let msg = format!(
            "The method `apply` is inserted. The auto insertion will be deprecated, please write `{prefix}{}.apply` explicitly.",
            self.text(name.name)
        );
        self.diags.warning(self.tpd.span(tree).point(), msg);
        Some(self.with_apply(tree, Pt::Any, ctx).map(|apply| self.adapt(apply, Pt::Type(pt), ctx)))
    }
}
