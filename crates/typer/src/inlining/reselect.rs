//! Selections in a retyped inline body: the members dotty's retyping would select instead, and
//! the prefixes a selection keeps.

use super::{Inlining, Unsupported};
use crate::tpd::TNodeId;
use tasty::tags::PRIVATE;
use tasty::{SymbolId, Type, TypeId};

impl Inlining<'_> {
    /// Makes selection `t` of `name` from retyped `qualifier` select the member dotty's retyping
    /// selects instead (`selectionType`), if there is one.
    pub fn reselect(&mut self, t: TNodeId, qualifier: TNodeId, name: String) -> Result<(), Unsupported> {
        let refused = (self.tpd.span(t), "inline expansion selecting an overriding member");
        if let Some(m) = self.reselected(t, qualifier, name).map_err(|()| refused)? {
            let prefix = self.tpd.tpe(qualifier);
            let tpe = crate::defn::ref_type(self.u, Some(prefix), m);
            let node = self.tpd.get_mut(t);
            (node.sym, node.tpe) = (Some(m), Some(tpe));
        }
        Ok(())
    }

    /// The member named `name` that selection `t` from `qualifier` finds in the class of the
    /// qualifier's type before the owner of the selected symbol with the selected symbol's
    /// signature: none if there is none, `Err` if there are several or it is private.
    fn reselected(&mut self, t: TNodeId, qualifier: TNodeId, name: String) -> Result<Option<SymbolId>, ()> {
        let (Some(sym), Some(q)) = (self.tpd.sym(t), self.tpd.get(qualifier).tpe) else { return Ok(None) };
        let owner = self.u.symbol(sym).owner;
        let widened = self.u.widen(q);
        let Some(c) = self.u.class_symbol(widened) else { return Ok(None) };
        for base in self.u.linearization(c) {
            if Some(base) == owner {
                return Ok(None);
            }
            let decls = self.u.decls(base).unwrap_or_default();
            let site = crate::defn::type_ref(self.u, c);
            let same: Vec<SymbolId> = (decls.into_iter())
                .filter(|&d| {
                    self.u.symbol(d).name == name
                        && self.u.symbol(d).kind.is_term()
                        && self.u.matches(site, d, sym)
                })
                .collect();
            let public = same.iter().all(|&d| !self.u.symbol(d).flags.has(PRIVATE));
            match same[..] {
                [] => continue,
                [d] if public => return Ok(Some(d)),
                _ => return Err(()),
            }
        }
        Ok(None)
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
