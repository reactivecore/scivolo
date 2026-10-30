//! Type projections `C#T` of a type alias `T`.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, TName};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Projection `id`, `qual#name`, of type alias `sym` of `alias`: the alias with the aliases
    /// of its owner it refers to expanded, dealiased, its class its symbol; refused if it refers
    /// to `this` or to another member of `sym`'s owner.
    pub fn alias_projection(
        &mut self,
        id: NodeId,
        (qual, name): (TNodeId, TName),
        sym: SymbolId,
        alias: TypeId,
    ) -> TNodeId {
        let owner = self.u.symbol(sym).owner;
        let Some(alias) = self.expand_owner_aliases(alias, owner) else {
            return self.unsupported(id, "projection of a type alias referring to its owner");
        };
        let tpe = self.u.dealias(alias);
        let class = match *self.u.types().get(tpe) {
            Type::Sym(_, s) => Some(s),
            _ => None,
        };
        let span = self.ast.span(id);
        self.tpd.node(T::Select { qualifier: qual, name }, span, Some(tpe), class)
    }

    /// `t` with each type alias of class `owner` it refers to replaced by its alias; `None` if it
    /// refers to `this` or to another member of `owner`.
    fn expand_owner_aliases(&mut self, mut t: TypeId, owner: Option<SymbolId>) -> Option<TypeId> {
        for _ in 0..64 {
            let (mut this, mut member) = (false, None);
            self.u.map_type(t, &mut |u, part| {
                match *u.types().get(part) {
                    Type::This(_) => this = true,
                    Type::Sym(_, s) if u.symbol(s).owner == owner => member = member.or(Some(s)),
                    _ => {}
                }
                None
            });
            let Some(member) = member else { return (!this).then_some(t) };
            self.complete(member);
            let alias = self.u.alias_of(member)?;
            t = self.u.map_type(t, &mut |u, part| match *u.types().get(part) {
                Type::Sym(_, s) if s == member => Some(alias),
                _ => None,
            });
        }
        None
    }
}
