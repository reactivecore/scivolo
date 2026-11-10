//! Type projections `C#T` of a type alias `T`.

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, TName};
use tasty::{SymbolId, Type, TypeId};

/// How many aliases of the owner `expand_owner_aliases` expands at most.
const MAX_EXPANSIONS: usize = 64;

impl Typer<'_> {
    /// Projection `id`, `qual#name`, of type alias `sym` of `alias`: the alias seen from `qual`
    /// with the aliases of its owner it refers to expanded, dealiased, its class its symbol;
    /// `qual#name` itself if it refers to `this`; refused if it refers to another member of
    /// `sym`'s owner.
    pub fn alias_projection(
        &mut self,
        id: NodeId,
        (qual, name): (TNodeId, TName),
        sym: SymbolId,
        alias: TypeId,
    ) -> TNodeId {
        let owner = self.u.symbol(sym).owner;
        let seen = match self.refers_to_this(alias) {
            true => alias,
            false => {
                let pre = self.tpd.tpe(qual);
                let info = self.u.member_info(pre, sym);
                match *self.u.types().get(info) {
                    Type::Alias(a) => a,
                    _ => alias,
                }
            }
        };
        let span = self.ast.span(id);
        let Some(alias) = self.expand_owner_aliases(seen, owner) else {
            if self.refers_to_member_of(seen, owner) {
                return self.unsupported(id, "projection of a type alias referring to its owner");
            }
            let pre = self.tpd.tpe(qual);
            let tpe = self.u.intern_type(Type::Sym(Some(pre), sym));
            return self.tpd.node(T::Select { qualifier: qual, name }, span, Some(tpe), Some(sym));
        };
        let tpe = self.u.dealias(alias);
        let class = match *self.u.types().get(tpe) {
            Type::Sym(_, s) => Some(s),
            _ => None,
        };
        self.tpd.node(T::Select { qualifier: qual, name }, span, Some(tpe), class)
    }

    /// Whether `t` refers to the `this` of a class other than to select a type parameter.
    fn refers_to_this(&mut self, t: TypeId) -> bool {
        let mut this = false;
        self.u.map_type(t, &mut |u, part| match *u.types().get(part) {
            Type::Sym(Some(_), s) if u.symbol(s).kind == tasty::Kind::TypeParam => Some(part),
            Type::This(c) => {
                this |= !matches!(u.types().get(c), Type::Package(_));
                None
            }
            _ => None,
        });
        this
    }

    /// Whether `t` refers to a member of `owner`.
    fn refers_to_member_of(&mut self, t: TypeId, owner: Option<SymbolId>) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            found |= matches!(*u.types().get(part), Type::Sym(_, s) if u.symbol(s).owner == owner);
            None
        });
        found
    }

    /// `t` with each type alias of class `owner` it refers to replaced by its alias; `None` if it
    /// refers to `this` or to another member of `owner`.
    fn expand_owner_aliases(&mut self, mut t: TypeId, owner: Option<SymbolId>) -> Option<TypeId> {
        for _ in 0..MAX_EXPANSIONS {
            let (mut this, mut member) = (false, None);
            self.u.map_type(t, &mut |u, part| {
                match *u.types().get(part) {
                    Type::This(c) => this |= !matches!(u.types().get(c), Type::Package(_)),
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
