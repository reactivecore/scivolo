//! Methods the phases add to a class: forwarders and bridges, each calling another method with
//! its own parameters, in one erased parameter list, and their type parameters.

use crate::definitions::symbol;
use crate::{Lower, Result};
use syntax::ast::{Span, TName};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

impl Lower<'_> {
    /// A method of class `class` with the name and target name of `like`, flags `flags` and type
    /// `info`, whose body `call` makes from references to its parameters, with the result type
    /// of `info`; its symbol and definition.
    pub fn method_like(
        &mut self,
        class: SymbolId,
        like: SymbolId,
        flags: Flags,
        info: tasty::TypeId,
        span: Span,
        call: impl FnOnce(&mut Self, &[TNodeId], tasty::TypeId) -> TNodeId,
    ) -> Result<(SymbolId, TNodeId)> {
        let name = self.u.symbol(like).name.clone();
        let mut s = symbol(&name, Kind::Def, class, flags);
        s.target = self.u.symbol(like).target.clone();
        let sym = self.u.define(s);
        self.u.set_info(sym, info);
        Ok((sym, self.forwarding_def(sym, span, call)))
    }

    /// The definition of method `sym`, whose body `call` makes from references to its
    /// parameters, with the result type of its type.
    pub fn forwarding_def(
        &mut self,
        sym: SymbolId,
        span: Span,
        call: impl FnOnce(&mut Self, &[TNodeId], tasty::TypeId) -> TNodeId,
    ) -> TNodeId {
        let info = self.u.info(sym);
        let mut params = Vec::new();
        let mut t = info;
        let mut has_clause = false;
        loop {
            match self.u.types().get(t) {
                Type::Poly { result, .. } => t = *result,
                Type::Method { params: ps, result, .. } => {
                    params.extend_from_slice(ps);
                    has_clause = true;
                    t = *result;
                }
                _ => break,
            }
        }
        let mut param_defs = Vec::new();
        let mut args = Vec::new();
        for (pname, ptype) in params {
            if self.u.is_erased_param(ptype) {
                continue;
            }
            let p = self.u.define(symbol(&pname, Kind::Param, sym, Default::default()));
            self.u.set_info(p, ptype);
            let tpt = self.type_tree(ptype, span);
            param_defs.push(self.build().val_def(p, tpt, None, span));
            args.push(self.ident(p, span));
        }
        let result = self.u.result_type(info);
        let rhs = call(self, &args, result);
        let paramss: &[&[TNodeId]] = if has_clause { &[&param_defs] } else { &[] };
        let tpt = self.type_tree(result, span);
        self.build().def_def(sym, paramss, tpt, Some(rhs), span)
    }

    /// The definition of type parameter `t`, at `at`.
    pub fn type_param_def(&mut self, t: SymbolId, at: Span) -> TNodeId {
        let name = self.u.symbol(t).name.clone();
        let name = TName { name: self.names.intern(&name), is_type: true };
        let info = self.u.info(t);
        let tpt = self.type_tree(info, at);
        self.tpd.node(Tree::TypeDef { name, rhs: tpt }, at, None, Some(t))
    }
}
