//! `new C` and the types of constructor calls (dotty's `typedNew`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::NodeId;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_new(&mut self, id: NodeId, tpt: NodeId, ctx: Ctx) -> TNodeId {
        let tpt = self.typed_type(tpt, ctx);
        let tpe = self.tpd.tpe(tpt);
        if let Type::Sym(_, c) = *self.u.types().get(tpe)
            && !self.u.type_params(c).is_empty()
        {
            return self.unsupported(id, "new of a generic class without type arguments");
        }
        let span = self.ast.span(id);
        self.tpd.node(T::New { tpt }, span, Some(tpe), None)
    }

    /// The type of a call of constructor `ctor` on `new` of type `class`, whose method type gives
    /// `result`: `result` if it names the constructor's class, else the class itself unless
    /// `class` applies it to arguments.
    pub fn constructor_result(&mut self, ctor: SymbolId, class: TypeId, result: TypeId) -> TypeId {
        let owner = self.u.symbol(ctor).owner;
        let names_owner = |u: &tasty::Universe, t| match *u.types().get(t) {
            Type::Sym(_, c) => Some(c) == owner,
            Type::Applied(c, _) => matches!(*u.types().get(c), Type::Sym(_, c) if Some(c) == owner),
            _ => false,
        };
        match owner {
            _ if names_owner(self.u, result) => result,
            Some(o) if !matches!(self.u.types().get(class), Type::Applied(..)) => {
                crate::defn::type_ref(self.u, o)
            }
            _ => class,
        }
    }
}
