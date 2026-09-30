//! The declared types of symbols, decoded from their trees on first use.

use super::{FileId, Kind, SymbolId, Universe};
use crate::tags::*;
use crate::types::{Binder, Decoder, Type, TypeId};
use crate::{NameRef, NodeId, Tasty, View, view};

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
        let tasty = Tasty::parse(&loaded.bytes).expect("parsed when loaded");
        let mut d = Decoder { file, ast: &loaded.ast, names: &tasty.names, by_tree, packages, types };
        let ast = &loaded.ast;
        match view(ast, id) {
            View::DefDef { parts, .. } => {
                let result = d.ty(parts.body[0]);
                clauses(ast, parts.params).into_iter().rev().fold(result, |result, clause| {
                    let binder = Binder(file, ast.node(clause[0]).addr);
                    let params = clause
                        .iter()
                        .filter(|&&p| matches!(ast.tag(p), PARAM | TYPEPARAM))
                        .map(|&p| (d.names.show(NameRef(ast.node(p).a as u32)), d.ty(ast.kids(p)[0])))
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
                    d.types.intern(t)
                })
            }
            _ => d.ty(ast.kids(id)[0]),
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
