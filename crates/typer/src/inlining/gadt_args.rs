//! Inline arguments and expansions cast by GADT bounds: the skolem of an argument, the casts
//! outside the subset.

use super::Inlining;
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use tasty::{Type, TypeId};

impl Inlining<'_> {
    /// The skolem by which the proxy of argument `arg` is typed: none for an argument the
    /// implicit search found and cast by GADT bounds, as `summon[A =:= (u, u)]` in a case that
    /// knows `A = (u, u)`, whose proxy dotty types by the cast type; `skolem` otherwise.
    pub fn gadt_arg_skolem(&self, arg: TNodeId, skolem: Option<TypeId>) -> Option<TypeId> {
        let T::TypeApply { fun, .. } = *self.tpd.tree(arg) else { return skolem };
        let T::Select { name, .. } = *self.tpd.tree(fun) else { return skolem };
        let span = self.tpd.span(arg);
        let inferred = span.start() == span.end();
        match inferred && self.names.text(name.name) == nme::AS_INSTANCE_OF_PM && self.u.has_gadt_bounds() {
            true => None,
            false => skolem,
        }
    }

    /// Refuses `t` if it casts a singleton by GADT bounds, as `x.asInstanceOf[x.type & Int & T]`
    /// in the body of `inline def f[T](x: T): T` with `T = Int`, whose type in an expansion dotty
    /// simplifies.
    pub fn check_gadt_cast(&mut self, t: TNodeId) -> Result<(), &'static str> {
        let T::TypeApply { fun, .. } = *self.tpd.tree(t) else { return Ok(()) };
        let is_cast = self.tpd.sym(fun).is_some_and(|m| self.u.symbol(m).name == nme::AS_INSTANCE_OF_PM);
        let mut target = self.tpd.tpe(t);
        while let Some((a, _)) = self.u.annotated(target) {
            target = a;
        }
        let Type::And(mut first, _) = *self.u.ty(target) else { return Ok(()) };
        while let Type::And(a, _) | Type::Annotated(a, _) = *self.u.ty(first) {
            first = a;
        }
        let singleton = self.u.sym_of(first).is_some_and(|s| self.u.symbol(s).kind.is_term())
            || matches!(
                self.u.ty(first),
                Type::Constant(..) | Type::Str(_) | Type::This(_) | Type::TermRef(..)
            );
        match is_cast && singleton {
            true => Err("GADT cast of a singleton in an inline expansion"),
            false => Ok(()),
        }
    }
}
