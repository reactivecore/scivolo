//! Calls the phases make, at the spans of their parts: `qual.m(args)`, `lhs == rhs` with the
//! overload dotty's `applyOverloaded` picks, Java static calls, `throw e`.

use super::casts::class_named;
use crate::Lower;
use crate::definitions::{primitive_class_name, type_ref};
use syntax::ast::Span;
use tasty::{Erased, SymbolId, Type, TypeId};
use typer::tpd::TNodeId;

impl Lower<'_> {
    /// `qual.m(args)`, of a method `m` with one parameter list.
    pub fn call(&mut self, qual: TNodeId, m: SymbolId, args: &[TNodeId]) -> TNodeId {
        let fun = self.select(qual, m, Span::NONE);
        let fun = self.positioned(fun);
        let info = self.u.info(m);
        let result = match *self.u.types().get(info) {
            Type::Method { result, .. } => result,
            _ => info,
        };
        let apply = self.apply(fun, args, result, Span::NONE);
        self.positioned(apply)
    }

    /// `lhs == rhs`, with the `==` of `lhs`'s class taking `rhs`'s class if any, else `Any`'s.
    pub fn equal(&mut self, lhs: TNodeId, rhs: TNodeId) -> TNodeId {
        let eq = self.operator(lhs, "==", rhs).unwrap_or(self.defs.any_eq);
        self.call(lhs, eq, &[rhs])
    }

    /// The operator `op` of `lhs`'s class taking `rhs`'s class.
    pub fn operator(&mut self, lhs: TNodeId, op: &str, rhs: TNodeId) -> Option<SymbolId> {
        let class = self.class_of(self.tpd.tpe(lhs));
        let arg = self.class_of(self.tpd.tpe(rhs));
        let ops = class.and_then(|c| self.u.decl(c, op).ok())?;
        ops.into_iter().find(|&m| self.single_param_class(m) == arg)
    }

    /// `m(args)` of Java static method `m`.
    pub fn static_call(&mut self, m: SymbolId, args: &[TNodeId]) -> TNodeId {
        let fun = self.ident(m, Span::NONE);
        let info = self.u.info(m);
        let result = self.u.result_type(info);
        let apply = self.apply(fun, args, result, Span::NONE);
        self.positioned(apply)
    }

    /// `throw e`.
    pub fn throw_(&mut self, e: TNodeId) -> TNodeId {
        let throw = self.defs.throw;
        let throw = self.ident(throw, Span::NONE);
        let nothing = self.u.builtins().nothing;
        let nothing = type_ref(self.u, nothing);
        let apply = self.apply(throw, &[e], nothing, Span::NONE);
        self.positioned(apply)
    }

    /// The class of the only parameter of method `m`.
    fn single_param_class(&mut self, m: SymbolId) -> Option<SymbolId> {
        let info = self.u.info(m);
        let Type::Method { params, .. } = self.u.types().get(info).clone() else { return None };
        let &[(_, p)] = params.as_slice() else { return None };
        self.class_of(p)
    }

    /// The class of type `tpe`, through singletons, unions and aliases to its erasure's.
    pub fn class_of(&mut self, tpe: TypeId) -> Option<SymbolId> {
        let widened = self.u.widen(tpe);
        if let Some(c) = self.u.class_symbol(widened) {
            return Some(c);
        }
        let name = match self.u.erase(tpe) {
            Erased::Prim(letter) => primitive_class_name(letter)?,
            Erased::Class(name) => name,
            _ => return None,
        };
        class_named(self, &name)
    }
}
