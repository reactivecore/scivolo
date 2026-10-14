//! The classes nested in a lifted local class (dotty's intermediate classes of `markFree`): their
//! methods read the values the local class captures from its fields, through their enclosing
//! instances (dotty's `proxyRef` through `outer.path`).

use super::explicit_outer::terms;
use super::local_free::{is_local, is_method};
use super::outer_fields::outer_path;
use super::walk::{is_class_def, subtrees};
use crate::{Lower, Result, unsupported};
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

/// Makes the references to `values` in the member classes defined in `stat` of local class `class` read
/// its `fields` through their enclosing instances; fails where not in methods.
pub fn redirect(
    l: &mut Lower,
    stat: TNodeId,
    class: SymbolId,
    values: &[SymbolId],
    fields: &[SymbolId],
) -> Result<()> {
    let member = |t: TNodeId| is_class_def(l.tpd, t) && l.tpd.sym(t).is_some_and(|c| !is_local(l, c));
    let nested: Vec<TNodeId> = subtrees(l.tpd, stat).into_iter().filter(|&t| member(t)).collect();
    for td in nested {
        let inner = l.tpd.sym(td).expect("a class definition's symbol");
        let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
        let Tree::Template { parents, body, .. } = *l.tpd.tree(template) else { unreachable!("a template") };
        let stats: Vec<TNodeId> = l.tpd.items(parents).iter().chain(l.tpd.items(body)).copied().collect();
        let stats: Vec<TNodeId> =
            stats.into_iter().filter(|&s| !matches!(l.tpd.tree(s), Tree::TypeDef { .. })).collect();
        for s in stats {
            let method = matches!(l.tpd.tree(s), Tree::DefDef { .. }) && is_method(l, l.tpd.sym(s));
            for t in terms(l, s, false) {
                let used = l.tpd.sym(t).filter(|_| matches!(l.tpd.tree(t), Tree::Ident { .. }));
                let Some(i) = used.and_then(|v| values.iter().position(|&w| w == v)) else { continue };
                let span = l.tpd.span(t);
                if !method {
                    return unsupported(span, "classes in local classes capturing values");
                }
                let outer = outer_path(l, (td, inner), class, true, span)?;
                let read = l.select(outer, fields[i], span);
                l.copy_node(t, read);
            }
        }
    }
    Ok(())
}
