//! Selections in a retyped inline body: the members dotty's retyping would select instead, and
//! the prefixes a selection keeps.

use super::Inlining;
use crate::tpd::TNodeId;
use tasty::{Type, TypeId};

impl Inlining<'_> {
    /// Whether selection `t` of member `name` from `qualifier` finds, in the class of the
    /// qualifier's type, a member of that name before the owner of the selected symbol, which
    /// dotty's retyping would select instead.
    pub fn reselects_override(&mut self, t: TNodeId, qualifier: TNodeId, name: String) -> bool {
        let (Some(sym), Some(q)) = (self.tpd.sym(t), self.tpd.get(qualifier).tpe) else { return false };
        let owner = self.u.symbol(sym).owner;
        let widened = self.u.widen(q);
        let Some(c) = self.u.class_symbol(widened) else { return false };
        for base in self.u.linearization(c) {
            if Some(base) == owner {
                return false;
            }
            let decls = self.u.decls(base).unwrap_or_default();
            if decls.iter().any(|&d| self.u.symbol(d).name == name && self.u.symbol(d).kind.is_term()) {
                return true;
            }
        }
        false
    }

    /// Whether `t` is a reference to a stable local term or a `this`.
    pub fn is_stable_ref(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::This(_) => true,
            Type::Sym(None, s) => self.u.symbol(s).kind.is_term() && self.is_stable_member(s),
            _ => false,
        }
    }
}
