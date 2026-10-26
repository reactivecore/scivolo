//! The class definitions of a unit and the spans of its classes and methods.

use crate::Lower;
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

/// The class definitions of the unit's packages and of their templates, each before those in it.
pub fn type_defs(l: &Lower, root: TNodeId) -> Vec<TNodeId> {
    let mut out = Vec::new();
    let mut todo = vec![root];
    while let Some(t) = todo.pop() {
        match *l.tpd.tree(t) {
            Tree::PackageDef { stats, .. } => todo.extend(l.tpd.items(stats).iter().rev()),
            Tree::TypeDef { rhs, .. } => {
                let Tree::Template { body, .. } = *l.tpd.tree(rhs) else { continue };
                out.push(t);
                todo.extend(l.tpd.items(body).iter().rev());
            }
            _ => {}
        }
    }
    out
}

/// The spans of classes `type_defs` and of the methods in their templates but those mixed in or
/// specialized.
pub fn definition_spans(l: &Lower, type_defs: &[TNodeId]) -> FxHashMap<SymbolId, Span> {
    let mut out = FxHashMap::default();
    for &td in type_defs {
        out.extend(l.tpd.sym(td).map(|s| (s, l.tpd.span(td))));
        let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { continue };
        let Tree::Template { body, .. } = *l.tpd.tree(rhs) else { continue };
        for &stat in l.tpd.items(body) {
            let sym = l.tpd.sym(stat).filter(|s| !l.mixed_in.contains(s) && !l.specialized.contains(s));
            out.extend(sym.map(|s| (s, l.tpd.span(stat))));
        }
    }
    out
}
