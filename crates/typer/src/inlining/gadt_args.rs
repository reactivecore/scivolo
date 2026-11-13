//! The skolem of an inline argument cast by GADT bounds.

use super::Inlining;
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use tasty::TypeId;

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
}
