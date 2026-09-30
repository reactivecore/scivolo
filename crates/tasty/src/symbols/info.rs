//! The declared types of symbols, decoded from their trees on first use.

use super::{FileId, Kind, SymbolId, Universe};
use crate::tags::*;
use crate::types::{Binder, Decoder, Type, TypeId};
use crate::{NodeId, View, view};

impl Universe {
    /// The declared type of `sym`: a method's type, a value's type, a type member's alias or
    /// bounds, a type parameter's bounds; `Other(TEMPLATE)` for classes and packages.
    pub fn info(&mut self, sym: SymbolId) -> TypeId {
        if let Some(&t) = self.infos.get(&sym) {
            return t;
        }
        let (kind, tree) = (self.symbol(sym).kind, self.symbol(sym).tree);
        let t = match (kind, tree) {
            (Kind::Package | Kind::Class, _) | (_, None) => self.types.intern(Type::Other(TEMPLATE)),
            (_, Some((file, id))) => self.decode_info(file, id),
        };
        self.infos.insert(sym, t);
        t
    }

    fn decode_info(&mut self, file: FileId, id: NodeId) -> TypeId {
        let Universe { files, by_tree, packages, types, .. } = self;
        let loaded = &files[file.0 as usize];
        let ast = &loaded.ast;
        let mut d = Decoder::new((file, ast, &loaded.names), by_tree, packages, types);
        match view(ast, id) {
            View::DefDef { parts, .. } => {
                let result = match parts.body.first() {
                    Some(&tpt) => d.ty(tpt),
                    None => d.intern(Type::Other(DEFDEF)),
                };
                clauses(ast, parts.params).into_iter().rev().fold(result, |result, clause| {
                    let binder = Binder(file, ast.node(clause[0]).addr);
                    let params = clause
                        .iter()
                        .filter(|&&p| matches!(ast.tag(p), PARAM | TYPEPARAM))
                        .map(|&p| (d.name(p), param_type(&mut d, ast, p)))
                        .collect();
                    let t = match ast.tag(clause[0]) {
                        TYPEPARAM => Type::Poly { binder, params, result },
                        _ => {
                            let implicit = clause.iter().any(|&p| {
                                ast.kids(p).iter().any(|&m| matches!(ast.tag(m), GIVEN | IMPLICIT))
                            });
                            Type::Method { binder, params, result, implicit }
                        }
                    };
                    d.intern(t)
                })
            }
            _ => param_type(&mut d, ast, id),
        }
    }
}

/// The type of the first child of the definition at `id`: a value's or parameter's type tree,
/// a type parameter's bounds, a type member's right-hand side.
fn param_type(d: &mut Decoder, ast: &crate::Ast, id: NodeId) -> TypeId {
    match ast.kids(id).first() {
        Some(&t) => d.ty(t),
        None => d.intern(Type::Other(ast.tag(id))),
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
