//! Methods the phases add to a class: forwarders and bridges, each calling another method with
//! its own parameters.

use crate::definitions::symbol;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::TNodeId;

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
        let params = match self.u.types().get(info).clone() {
            Type::Method { params, result, .. }
                if !matches!(self.u.types().get(result), Type::Method { .. }) =>
            {
                params
            }
            Type::Method { .. } | Type::Poly { .. } => {
                return unsupported(span, "added methods with type parameters or several parameter lists");
            }
            _ => Vec::new(),
        };
        let name = self.u.symbol(like).name.clone();
        let mut s = symbol(&name, Kind::Def, class, flags);
        s.target = self.u.symbol(like).target.clone();
        let sym = self.u.define(s);
        self.u.set_info(sym, info);
        let mut param_defs = Vec::new();
        let mut args = Vec::new();
        for (pname, ptype) in params {
            let p = self.u.define(symbol(&pname, Kind::Param, sym, Default::default()));
            self.u.set_info(p, ptype);
            let tpt = self.type_tree(ptype, span);
            param_defs.push(self.build().val_def(p, tpt, None, span));
            args.push(self.ident(p, span));
        }
        let result = self.u.result_type(info);
        let rhs = call(self, &args, result);
        let paramss: &[&[TNodeId]] = match self.u.types().get(info) {
            Type::Method { .. } => &[&param_defs],
            _ => &[],
        };
        let tpt = self.type_tree(result, span);
        let def = self.build().def_def(sym, paramss, tpt, Some(rhs), span);
        Ok((sym, def))
    }
}
