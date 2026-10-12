//! The inline method read with its parameters (dotty's `readIndexedDef`), and its blocks, which
//! may not define locals yet.

use super::{Refusal, Unpickler};
use crate::tpd::{Build, TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::tags::*;
use tasty::{NodeId, View, view};

impl Unpickler<'_> {
    /// The definition of the inline method at `id`.
    pub fn inline_def(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let View::DefDef { parts, .. } = view(self.ast(), id) else {
            return Err("inline method of another kind");
        };
        let (params, body) = (parts.params.to_vec(), parts.body.to_vec());
        let mut paramss = Vec::new();
        for clause in tasty::param_tree_clauses(self.ast(), &params) {
            let params: Vec<NodeId> =
                clause.into_iter().filter(|&p| matches!(self.tag(p), PARAM | TYPEPARAM)).collect();
            paramss.push(params.into_iter().map(|p| self.param(p)).collect::<Result<Vec<_>, _>>()?);
        }
        let (Some(&tpt), Some(&rhs)) = (body.first(), body.get(1)) else {
            return Err("inline method without a body");
        };
        let (tpt, rhs) = (self.tpt(tpt)?, self.term(rhs)?);
        let clauses: Vec<&[TNodeId]> = paramss.iter().map(Vec::as_slice).collect();
        let unit = self.class_symbol("scala.Unit")?;
        let def = Build::new(self.tpd, self.u, self.names, unit).def_def(
            self.meth,
            &clauses,
            tpt,
            Some(rhs),
            Span::NONE,
        );
        Ok(self.positioned(id, def))
    }

    /// The parameter defined at `id`, its type tree the type it is entered with.
    fn param(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let sym =
            self.u.defined_at(self.file, id).ok_or("parameter without a symbol in a library inline body")?;
        let info = self.u.info(sym);
        self.check(info)?;
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

    /// `BLOCK` of statements that are terms.
    pub fn block(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        for &s in &k[1..] {
            match self.tag(s) {
                VALDEF | DEFDEF | TYPEDEF => return Err("local definition in a library inline body"),
                IMPORT | EXPORT => return Err("import in a library inline body"),
                _ => {}
            }
        }
        let stats = k[1..].iter().map(|&s| self.term(s)).collect::<Result<Vec<_>, _>>()?;
        let expr = self.term(k[0])?;
        let tpe = self.tpd.get(expr).tpe;
        let stats = self.tpd.list(&stats);
        Ok(self.bare(T::Block { stats, expr }, tpe, None))
    }
}
