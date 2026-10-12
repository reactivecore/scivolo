//! Definitions: the inline method read with its parameters, and the local values and methods of
//! its blocks, entered when their block is read (dotty's `readIndexedDef`, `readStats`).

use super::{Refusal, Unpickler};
use crate::tpd::{Build, TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::tags::*;
use tasty::{NodeId, SymbolId, View, view};

impl Unpickler<'_> {
    /// The definition of the inline method at `id`.
    pub fn inline_def(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let def = self.def_def(id, self.meth)?;
        match self.tpd.tree(def) {
            T::DefDef { rhs: Some(_), .. } => Ok(def),
            _ => Err("inline method without a body"),
        }
    }

    /// The definition at `id` of method `sym`, its body owning the locals in it.
    fn def_def(&mut self, id: NodeId, sym: SymbolId) -> Result<TNodeId, Refusal> {
        let View::DefDef { parts, .. } = view(self.ast(), id) else {
            return Err("definition of another kind in a library inline body");
        };
        let (params, body) = (parts.params.to_vec(), parts.body.to_vec());
        let mut paramss = Vec::new();
        for clause in tasty::param_tree_clauses(self.ast(), &params) {
            let params: Vec<NodeId> =
                clause.into_iter().filter(|&p| matches!(self.tag(p), PARAM | TYPEPARAM)).collect();
            paramss.push(params.into_iter().map(|p| self.param(p)).collect::<Result<Vec<_>, _>>()?);
        }
        let tpt = self.tpt(*body.first().ok_or("method without a result type in a library inline body")?)?;
        let outer = std::mem::replace(&mut self.owner, sym);
        let rhs = body.get(1).map(|&r| self.term(r)).transpose();
        self.owner = outer;
        let clauses: Vec<&[TNodeId]> = paramss.iter().map(Vec::as_slice).collect();
        let unit = self.class_symbol("scala.Unit")?;
        let def =
            Build::new(self.tpd, self.u, self.names, unit).def_def(sym, &clauses, tpt, rhs?, Span::NONE);
        Ok(self.positioned(id, def))
    }

    /// The parameter defined at `id`, its type tree the type it is entered with.
    fn param(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let sym =
            self.u.defined_at(self.file, id).ok_or("parameter without a symbol in a library inline body")?;
        let info = self.u.info(sym);
        let tpt = self.bare(T::TypeTree { inferred: false }, Some(info), None);
        let tpt = self.positioned(self.kids(id)[0], tpt);
        let is_type = self.tag(id) == TYPEPARAM;
        let name = self.tname(id, is_type);
        let tree = match is_type {
            true => T::TypeDef { name, rhs: tpt },
            false => T::ValDef { name, tpt, rhs: None },
        };
        let t = self.bare(tree, None, Some(sym));
        Ok(self.positioned(id, t))
    }

    /// `BLOCK`: its local definitions entered first, then its statements and expression read.
    pub fn block(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        let mut locals = Vec::new();
        for &s in &k[1..] {
            if matches!(self.tag(s), VALDEF | DEFDEF) {
                let local = self.u.enter_local(self.file, s, self.owner);
                locals.push(local.ok_or("local definition without a symbol in a library inline body")?);
            }
        }
        let stats = k[1..].iter().map(|&s| self.stat(s)).collect::<Result<Vec<_>, _>>()?;
        let expr = self.term(k[0])?;
        let tpe = self.tpd.tpe(expr);
        let tpe = self.block_type(tpe, &locals)?;
        let stats = self.tpd.list(&stats);
        Ok(self.bare(T::Block { stats, expr }, Some(tpe), None))
    }

    fn stat(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let sym = self.u.defined_at(self.file, id);
        match (self.tag(id), sym) {
            (VALDEF, Some(sym)) => self.val_def(id, sym),
            (DEFDEF, Some(sym)) => self.def_def(id, sym),
            (TYPEDEF, _) => Err("local type or class in a library inline body"),
            (IMPORT | EXPORT, _) => Err("import in a library inline body"),
            _ => self.term(id),
        }
    }

    /// The definition at `id` of local value `sym`.
    fn val_def(&mut self, id: NodeId, sym: SymbolId) -> Result<TNodeId, Refusal> {
        let View::ValDef { tpt, rhs, .. } = view(self.ast(), id) else {
            return Err("value definition of another kind in a library inline body");
        };
        let tpt = self.tpt(tpt)?;
        let outer = std::mem::replace(&mut self.owner, sym);
        let rhs = rhs.map(|r| self.term(r)).transpose();
        self.owner = outer;
        let name = self.tname(id, false);
        let t = self.bare(T::ValDef { name, tpt, rhs: rhs? }, None, Some(sym));
        Ok(self.positioned(id, t))
    }
}
