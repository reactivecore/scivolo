//! Calls of lifted local methods: they call the lifted method with the captured values first; a
//! reference to a local method without parameter lists becomes such a call.

use super::lambda_lift::Function;
use super::lifted::Method;
use super::walk::children;
use crate::Lower;
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Makes the calls of the local methods among `functions` under `t`, and the references to those
/// without parameter lists, call their `methods` with the captured values first.
pub fn calls(l: &mut Lower, class: SymbolId, t: TNodeId, functions: &[Function], methods: &[Method]) {
    let mut todo = vec![t];
    while let Some(t) = todo.pop() {
        let (reference, args) = match *l.tpd.tree(t) {
            Tree::Apply { fun, args } => (fun, l.tpd.items(args).to_vec()),
            _ => (t, Vec::new()),
        };
        let lifted = l.tpd.sym(reference).and_then(|s| functions.iter().position(|f| f.sym == s));
        if let Some(i) = lifted.filter(|&i| functions[i].closure.is_none())
            && matches!(l.tpd.tree(reference), Tree::Ident { .. })
            && (reference != t || is_parameterless(l, functions[i].def))
        {
            let span = l.tpd.span(reference);
            let this = l.this(class, span);
            let fun = l.select(this, methods[i].sym, span);
            let captured: Vec<TNodeId> = functions[i].captured.iter().map(|&s| l.ident(s, span)).collect();
            let all: Vec<TNodeId> = captured.into_iter().chain(args).collect();
            let args = l.tpd.list(&all);
            if reference == t {
                let info = l.u.info(functions[i].sym);
                let result = if let Type::ByName(r) = *l.u.types().get(info) { r } else { info };
                (l.tpd.get_mut(t).sym, l.tpd.get_mut(t).tpe) = (None, Some(result));
            }
            l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
        }
        if !matches!(l.tpd.tree(t), Tree::TypeDef { .. }) {
            todo.extend(children(l.tpd, t));
        }
    }
}

/// Whether method definition `def` has no parameter lists.
fn is_parameterless(l: &Lower, def: TNodeId) -> bool {
    matches!(*l.tpd.tree(def), Tree::DefDef { paramss, .. } if l.tpd.clause_items(paramss).is_empty())
}
