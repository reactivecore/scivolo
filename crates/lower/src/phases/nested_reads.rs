// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Dependencies.scala
//   compiler/src/dotty/tools/dotc/transform/LambdaLift.scala
//! The classes nested in a lifted local class (dotty's intermediate classes of `markFree`): their
//! methods read the values the local class captures from its fields, through their enclosing
//! instances (dotty's `proxyRef` through `outer.path`).

use super::explicit_outer::outer_chain;
use super::local_free::{is_local, is_method, is_term};
use super::outer_fields::outer_path;
use super::walk::{children, is_class_def, is_type_tree, subtrees};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

/// The values each lifted local class captures, in its constructor and in fields, and the local
/// classes named in another one.
pub struct Nested {
    pub own: FxHashMap<SymbolId, (Vec<SymbolId>, Vec<SymbolId>)>,
    pub named_in: FxHashMap<SymbolId, SymbolId>,
}

/// Makes the references to `values` in the classes defined in `stat` of local class `class`
/// read its `fields` through their enclosing instances, in local functions through `this` of
/// `class`, which they are then lifted into; but not those to the values `own` lists for a nested
/// local class, which captures them itself; fails where not in methods. The local classes that
/// read fields are named in `class` (dotty's `narrowLogicOwner`), put in `named_in`.
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
        let (in_ctor, in_fields) = own.get(&inner).map_or((&[][..], &[][..]), |(c, f)| (c, f));
        let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
        let Tree::Template { parents, body, .. } = *l.tpd.tree(template) else { unreachable!("a template") };
        let stats: Vec<TNodeId> = l.tpd.items(parents).iter().chain(l.tpd.items(body)).copied().collect();
        let stats: Vec<TNodeId> =
            stats.into_iter().filter(|&s| !matches!(l.tpd.tree(s), Tree::TypeDef { .. })).collect();
        for s in stats {
            let method = matches!(l.tpd.tree(s), Tree::DefDef { .. }) && is_method(l, l.tpd.sym(s));
            for (t, functions) in in_functions(l, s) {
                let used = l.tpd.sym(t).filter(|_| matches!(l.tpd.tree(t), Tree::Ident { .. }));
                let used = used.filter(|v| !in_fields.contains(v) && (method || !in_ctor.contains(v)));
                let Some(i) = used.and_then(|v| values.iter().position(|&w| w == v)) else { continue };
                let span = l.tpd.span(t);
                let chain = outer_chain(l, inner, class).unwrap_or_default();
                let reachable = chain.split_last().is_some_and(|(_, before)| {
                    before.iter().all(|c| l.outer_accessors.contains_key(c) && !is_local(l, *c))
                });
                if !method || !reachable {
                    return unsupported(span, "classes in local classes capturing values");
                }
                let outer = match functions.is_empty() {
                    true => outer_path(l, (td, inner), class, true, span)?,
                    false => l.this(class, span),
                };
                l.narrowed.extend(functions.into_iter().map(|f| (f, class)));
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

/// The trees in `stat` but those in type trees and nested classes, each with the local functions
/// it is in, outermost first.
fn in_functions(l: &Lower, stat: TNodeId) -> Vec<(TNodeId, Vec<SymbolId>)> {
    let mut out = Vec::new();
    let mut todo = vec![(stat, Vec::new())];
    while let Some((t, mut functions)) = todo.pop() {
        if is_type_tree(l, t) || t != stat && matches!(l.tpd.tree(t), Tree::TypeDef { .. }) {
            continue;
        }
        let local = l.tpd.sym(t).filter(|&f| l.u.symbol(f).owner.is_some_and(|o| is_term(l, o)));
        if let (Tree::DefDef { .. }, Some(f)) = (l.tpd.tree(t), local) {
            functions.push(f);
        }
        todo.extend(children(l.tpd, t).into_iter().map(|c| (c, functions.clone())));
        out.push((t, functions));
    }
    out
}
