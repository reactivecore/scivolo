//! The classes nested in a lifted local class (dotty's intermediate classes of `markFree`): their
//! methods read the values the local class captures from its fields, through their enclosing
//! instances (dotty's `proxyRef` through `outer.path`).

use super::explicit_outer::{outer_chain, terms};
use super::local_free::{is_local, is_method};
use super::outer_fields::outer_path;
use super::walk::{is_class_def, subtrees};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

/// The values each lifted local class captures, and the local classes named in another one.
pub struct Nested {
    pub own: FxHashMap<SymbolId, Vec<SymbolId>>,
    pub named_in: FxHashMap<SymbolId, SymbolId>,
}

/// Makes the references to `values` in the classes defined in `stat` of local class `class`
/// read its `fields` through their enclosing instances, but those to the values `own` lists for
/// a nested local class, which captures them itself; fails where not in methods. The local
/// classes that read fields are named in `class` (dotty's `narrowLogicOwner`), put in `named_in`.
pub fn redirect(
    l: &mut Lower,
    stat: TNodeId,
    class: SymbolId,
    (values, fields): (&[SymbolId], &[SymbolId]),
    Nested { own, named_in }: &mut Nested,
) -> Result<()> {
    let nested: Vec<TNodeId> =
        subtrees(l.tpd, stat).into_iter().filter(|&t| is_class_def(l.tpd, t)).collect();
    for td in nested {
        let inner = l.tpd.sym(td).expect("a class definition's symbol");
        let captured = own.get(&inner).map_or(&[][..], Vec::as_slice);
        let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
        let Tree::Template { parents, body, .. } = *l.tpd.tree(template) else { unreachable!("a template") };
        let stats: Vec<TNodeId> = l.tpd.items(parents).iter().chain(l.tpd.items(body)).copied().collect();
        let stats: Vec<TNodeId> =
            stats.into_iter().filter(|&s| !matches!(l.tpd.tree(s), Tree::TypeDef { .. })).collect();
        for s in stats {
            let method = matches!(l.tpd.tree(s), Tree::DefDef { .. }) && is_method(l, l.tpd.sym(s));
            for t in terms(l, s, false) {
                let used = l.tpd.sym(t).filter(|_| matches!(l.tpd.tree(t), Tree::Ident { .. }));
                let used = used.filter(|v| !captured.contains(v));
                let Some(i) = used.and_then(|v| values.iter().position(|&w| w == v)) else { continue };
                let span = l.tpd.span(t);
                let chain = outer_chain(l, inner, class).unwrap_or_default();
                let reachable = chain.split_last().is_some_and(|(_, before)| {
                    before.iter().all(|c| l.outer_accessors.contains_key(c) && !is_local(l, *c))
                });
                if !method || !reachable {
                    return unsupported(span, "classes in local classes capturing values");
                }
                let outer = outer_path(l, (td, inner), class, true, span)?;
                let read = l.select(outer, fields[i], span);
                l.copy_node(t, read);
                if is_local(l, inner) {
                    named_in.insert(inner, class);
                }
            }
        }
    }
    Ok(())
}
