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

    /// The prefix of a selection from a qualifier of type `t` (dotty's `cpy.Select`): `t` if it is
    /// a reference to a stable local term or a `this`, the result type if it is one to a by-name
    /// local term.
    pub fn local_prefix(&mut self, t: TypeId) -> Option<TypeId> {
        match *self.u.types().get(t) {
            Type::This(_) => Some(t),
            Type::Sym(None, s) if self.u.symbol(s).kind.is_term() => {
                if self.is_stable_member(s) {
                    return Some(t);
                }
                let info = self.u.info(s);
                match *self.u.types().get(info) {
                    Type::ByName(r) => Some(r),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
