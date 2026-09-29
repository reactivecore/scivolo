//! Named parameters of function types and dependent type definitions.

use super::super::Parser;
use crate::ast::modifiers::Modifiers;
use crate::ast::{NodeId, TName};
use crate::source::Offset;

impl Parser<'_> {
    /// `[erased] id : Type`.
    pub fn typed_fun_param(&mut self, start: Offset, name: TName, mods: Modifiers) -> NodeId {
        self.accept_colon();
        let tpt = self.typ();
        let p = self.make_parameter(name, tpt, mods, false);
        self.at_span1(start, p)
    }

    /// `(id: Type, ...)`.
    pub fn fun_param_clause(&mut self) -> Vec<NodeId> {
        self.in_parens_with_commas(|p| {
            p.comma_separated(|p| {
                let (o, name) = (p.offset(), p.ident());
                p.typed_fun_param(o, name, Modifiers::default())
            })
        })
    }
}
