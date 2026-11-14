// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/typer/TypeAssigner.scala
//! The types of trees re-derived from their parts as the body of an inline method of a class is
//! copied (dotty's `TypedTreeCopier`).

use super::super::Typer;
use super::MakeInlineable;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{NOTHING, Type, TypeId};

/// The parts a tree's type is derived from, with the types they had before the tree was copied.
pub struct Parts {
    /// The tree's type.
    before: Option<TypeId>,
    /// A block's expression, a selection's qualifier, a conditional's branches, a match's cases,
    /// a `try`'s expression and cases, an inlined call's expansion.
    nodes: Vec<TNodeId>,
    /// The types of `nodes`, of a case its body's.
    types: Vec<Option<TypeId>>,
}

impl Parts {
    /// Takes the parts of `tree` and their types before it is copied.
    pub fn of(t: &Typer<'_>, tree: TNodeId) -> Parts {
        let nodes = match *t.tpd.tree(tree) {
            T::Block { expr: c, .. } | T::Select { qualifier: c, .. } | T::Inlined { expansion: c, .. } => {
                vec![c]
            }
            T::If { thenp, elsep, .. } => vec![thenp, elsep],
            T::Match { cases, .. } => t.tpd.items(cases).to_vec(),
            T::Try { expr, cases, .. } => [&[expr][..], t.tpd.items(cases)].concat(),
            _ => Vec::new(),
        };
        let types = nodes.iter().map(|&n| part_type(t, n)).collect();
        Parts { before: t.tpd.get(tree).tpe, nodes, types }
    }
}

/// The type of part `n`, of a case its body's.
fn part_type(t: &Typer<'_>, n: TNodeId) -> Option<TypeId> {
    match *t.tpd.tree(n) {
        T::CaseDef { body, .. } => t.tpd.get(body).tpe,
        _ => t.tpd.get(n).tpe,
    }
}

impl MakeInlineable {
    /// Re-derives the type of copied `tree` from its `parts` if a part's type changed: a block or
    /// inlined call takes its expression's type, as `{ 23.asInstanceOf[23 & Int & T] }` whose
    /// type the GADT bounds simplified to `23`, unless it refers to the block's definitions; a
    /// selection is taken on its qualifier's new type, widened if unstable; a conditional, match
    /// or `try` takes the lub of its branches. A block or conditional whose type is none of its
    /// parts' is re-derived even if they kept their types.
    pub fn retype(&self, t: &mut Typer<'_>, tree: TNodeId, parts: Parts) {
        let owner = t.u.symbol(self.meth).owner;
        if parts.nodes.is_empty() || !owner.is_some_and(|o| t.u.is_class(o)) {
            return;
        }
        let new: Vec<_> = parts.nodes.iter().map(|&n| part_type(t, n)).collect();
        let changed = new != parts.types;
        let derived = parts.types.contains(&parts.before);

        match *t.tpd.tree(tree) {
            T::Block { stats, .. } if changed || !derived => {
                let Some(new) = new[0] else { return };
                let locals: Vec<_> = t.tpd.items(stats).iter().filter_map(|&s| t.tpd.sym(s)).collect();
                if derived || !t.refers_to(new, &locals) {
                    t.tpd.get_mut(tree).tpe = Some(new);
                }
            }
            T::Inlined { bindings, .. } if changed => {
                let Some(new) = new[0] else { return };
                let locals: Vec<_> = t.tpd.items(bindings).iter().filter_map(|&s| t.tpd.sym(s)).collect();
                if !t.refers_to(new, &locals) {
                    t.tpd.get_mut(tree).tpe = Some(new);
                }
            }
            T::Select { .. } if changed => {
                let Some(new) = new[0].filter(|&n| n != NOTHING) else { return };
                let Some((Some(_), m)) = parts.before.and_then(|b| t.u.sym_ref(b)) else { return };
                let prefix = t.widen_if_unstable(new);
                t.tpd.get_mut(tree).tpe = Some(t.u.intern_type(Type::Sym(Some(prefix), m)));
            }
            T::If { .. } | T::Match { .. } | T::Try { .. }
                if changed || !derived && matches!(t.tpd.tree(tree), T::If { .. }) =>
            {
                let lub = new.iter().flatten().fold(NOTHING, |acc, &b| {
                    let b = t.zonk(b);
                    t.u.lub_in(acc, b, &mut t.constraint)
                });
                t.tpd.get_mut(tree).tpe = Some(lub);
            }
            _ => {}
        }
    }
}
