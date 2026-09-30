//! The types of definitions: a method's type from its parameter clauses, the members of a
//! refinement, the bounds of an opaque type.

use super::Decoder;
use crate::tags::*;
use crate::types::{Binder, Type, TypeId};
use crate::{NodeId, Parts, View, view};

impl Decoder<'_> {
    /// The type of the method with definition parts `parts`: its parameter clauses around its
    /// result type.
    pub fn def_type(&mut self, parts: &Parts) -> TypeId {
        let ast = self.ast;
        let result = match parts.body.first() {
            Some(&tpt) => self.ty(tpt),
            None => self.intern(Type::Other(DEFDEF)),
        };
        clauses(ast, parts.params).into_iter().rev().fold(result, |result, clause| {
            let binder = Binder(self.file, ast.node(clause[0]).addr);
            let params = clause
                .iter()
                .filter(|&&p| matches!(ast.tag(p), PARAM | TYPEPARAM))
                .map(|&p| (self.name(p), self.kid(p, 0)))
                .collect();
            let t = match ast.tag(clause[0]) {
                TYPEPARAM => Type::Poly { binder, params, result },
                _ => {
                    let implicit = clause
                        .iter()
                        .any(|&p| ast.kids(p).iter().any(|&m| matches!(ast.tag(m), GIVEN | IMPLICIT)));
                    Type::Method { binder, params, result, implicit }
                }
            };
            self.intern(t)
        })
    }

    /// The type of the definition at `id`: a method's type, else the type of its first child
    /// (a value's type tree, a type's right-hand side or bounds).
    pub fn definition_type(&mut self, id: NodeId) -> TypeId {
        match view(self.ast, id) {
            View::DefDef { parts, .. } => self.def_type(&parts),
            _ => self.kid(id, 0),
        }
    }

    /// `parent` refined by the member definitions `members`.
    pub fn refined(&mut self, parent: NodeId, members: &[NodeId]) -> TypeId {
        let mut t = self.ty(parent);
        for &m in members {
            if matches!(self.ast.tag(m), VALDEF | DEFDEF | TYPEDEF) {
                let info = self.definition_type(m);
                t = self.intern(Type::Refined(t, self.name(m), info));
            }
        }
        t
    }

    /// The bounds of the opaque type defined at `id`, when it states them.
    pub fn opaque_bounds(&mut self, id: NodeId) -> Option<(TypeId, TypeId)> {
        let ast = self.ast;
        let rhs = ast.deref(*ast.kids(id).first()?);
        let kids: Vec<NodeId> = ast.kids(rhs).iter().copied().filter(|&k| !is_modifier(ast.tag(k))).collect();
        match (ast.tag(rhs), &kids[..]) {
            (TYPEBOUNDStpt, [lo, hi, _, ..]) => Some((self.ty(*lo), self.ty(*hi))),
            _ => None,
        }
    }
}

/// The parameter clauses of a method's parameter trees: runs of type parameters, runs of value
/// parameters ended by a type parameter or `SPLITCLAUSE`, and empty clauses.
fn clauses(ast: &crate::Ast, params: &[NodeId]) -> Vec<Vec<NodeId>> {
    let mut out: Vec<Vec<NodeId>> = Vec::new();
    let mut prev = None;
    for &p in params {
        let tag = ast.tag(p);
        match tag {
            EMPTYCLAUSE => out.push(vec![p]),
            SPLITCLAUSE => {}
            _ if prev == Some(tag) => out.last_mut().expect("a clause").push(p),
            _ => out.push(vec![p]),
        }
        prev = Some(tag);
    }
    out
}
