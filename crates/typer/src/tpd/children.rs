//! The subtrees of typed trees, and trees rebuilt with mapped subtrees (dotty's `TreeMap`).

use super::{Field, List, TNodeId, Tpd};

impl Tpd {
    /// The direct subtrees of `t`, in order.
    pub fn children(&self, t: TNodeId) -> Vec<TNodeId> {
        let mut out = Vec::new();
        self.tree(t).clone().fields(&mut |field| match field {
            Field::Node(c) => out.push(*c),
            Field::Opt(c) => out.extend(*c),
            Field::List(l) => out.extend_from_slice(self.items(*l)),
            Field::Clauses(c) => out.extend(self.clause_items(*c).iter().flat_map(|&l| self.items(l))),
            Field::Name(_) | Field::OptName(_) | Field::Constant(_) | Field::Class(_) => {}
        });
        out
    }
}

impl AsMut<Tpd> for Tpd {
    fn as_mut(&mut self) -> &mut Tpd {
        self
    }
}

/// A child mapping: the new tree for a child, given the context and the child.
pub type MapChild<'f, C> = dyn FnMut(&mut C, TNodeId) -> TNodeId + 'f;

/// Replaces each direct subtree `c` of `t` by `f(cx, c)`, in the order of `Tpd::children`, and
/// rebuilds `t` in place if one of them changed; whether one did.
pub fn map_children<C: AsMut<Tpd> + ?Sized>(cx: &mut C, t: TNodeId, f: &mut MapChild<C>) -> bool {
    let mut tree = cx.as_mut().tree(t).clone();
    let mut changed = false;
    tree.fields(&mut |field| match field {
        Field::Node(c) => changed |= map_one(cx, c, f),
        Field::Opt(c) => changed |= c.as_mut().is_some_and(|c| map_one(cx, c, f)),
        Field::List(l) => changed |= map_list(cx, l, f),
        Field::Clauses(cs) => {
            let mut lists = cx.as_mut().clause_items(*cs).to_vec();
            let mut any = false;
            for l in &mut lists {
                any |= map_list(cx, l, f);
            }
            if any {
                *cs = cx.as_mut().clauses(&lists);
                changed = true;
            }
        }
        Field::Name(_) | Field::OptName(_) | Field::Constant(_) | Field::Class(_) => {}
    });
    if changed {
        cx.as_mut().get_mut(t).tree = tree;
    }
    changed
}

fn map_one<C: AsMut<Tpd> + ?Sized>(cx: &mut C, c: &mut TNodeId, f: &mut MapChild<C>) -> bool {
    let new = f(cx, *c);
    std::mem::replace(c, new) != new
}

fn map_list<C: AsMut<Tpd> + ?Sized>(cx: &mut C, l: &mut List, f: &mut MapChild<C>) -> bool {
    let mut items = cx.as_mut().items(*l).to_vec();
    let mut any = false;
    for c in &mut items {
        any |= map_one(cx, c, f);
    }
    if any {
        *l = cx.as_mut().list(&items);
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tpd::Tree;
    use syntax::ast::{Span, TName};
    use syntax::names::Names;

    fn leaf(tpd: &mut Tpd) -> TNodeId {
        tpd.node(Tree::TypeTree { inferred: true }, Span::NONE, None, None)
    }

    fn name() -> TName {
        TName { name: Names::default().intern("x"), is_type: false }
    }

    /// Maps `from` to `to` and keeps every other child.
    fn swap(tpd: &mut Tpd, t: TNodeId, from: TNodeId, to: TNodeId) -> bool {
        map_children(tpd, t, &mut |_, c| if c == from { to } else { c })
    }

    #[test]
    fn keeps_a_tree_whose_children_do_not_change() {
        let mut tpd = Tpd::default();
        let (a, b) = (leaf(&mut tpd), leaf(&mut tpd));
        let stats = tpd.list(&[a]);
        let block = tpd.node(Tree::Block { stats, expr: b }, Span::NONE, None, None);
        let mut seen = Vec::new();
        assert!(!map_children(&mut tpd, block, &mut |_, c| {
            seen.push(c);
            c
        }));
        assert_eq!(seen, [a, b]);
        assert_eq!(*tpd.tree(block), Tree::Block { stats, expr: b });
    }

    #[test]
    fn rebuilds_nodes_and_lists_in_place() {
        let mut tpd = Tpd::default();
        let (a, b, c) = (leaf(&mut tpd), leaf(&mut tpd), leaf(&mut tpd));
        let stats = tpd.list(&[a, b]);
        let block = tpd.node(Tree::Block { stats, expr: b }, Span::NONE, None, None);
        assert!(swap(&mut tpd, block, a, c));
        let Tree::Block { stats: new, expr } = *tpd.tree(block) else { unreachable!() };
        assert_eq!((tpd.items(new), expr), (&[c, b][..], b));
        assert_eq!(tpd.items(stats), [a, b]);
        assert!(swap(&mut tpd, block, b, a));
        assert_eq!(tpd.children(block), [c, a, a]);
    }

    #[test]
    fn maps_present_optional_children_only() {
        let mut tpd = Tpd::default();
        let (tpt, rhs, new) = (leaf(&mut tpd), leaf(&mut tpd), leaf(&mut tpd));
        let name = name();
        let none = tpd.node(Tree::ValDef { name, tpt, rhs: None }, Span::NONE, None, None);
        let some = tpd.node(Tree::ValDef { name, tpt, rhs: Some(rhs) }, Span::NONE, None, None);
        assert!(!swap(&mut tpd, none, rhs, new));
        assert_eq!(tpd.children(none), [tpt]);
        assert!(swap(&mut tpd, some, rhs, new));
        assert_eq!(*tpd.tree(some), Tree::ValDef { name, tpt, rhs: Some(new) });
    }

    #[test]
    fn rebuilds_parameter_clauses() {
        let mut tpd = Tpd::default();
        let (p, q, tpt, new) = (leaf(&mut tpd), leaf(&mut tpd), leaf(&mut tpd), leaf(&mut tpd));
        let (first, second) = (tpd.list(&[p]), tpd.list(&[q]));
        let paramss = tpd.clauses(&[first, second]);
        let def = Tree::DefDef { name: name(), paramss, tpt, rhs: None };
        let def = tpd.node(def, Span::NONE, None, None);
        assert_eq!(tpd.children(def), [p, q, tpt]);
        assert!(!swap(&mut tpd, def, new, p));
        assert!(swap(&mut tpd, def, q, new));
        let Tree::DefDef { paramss: changed, .. } = *tpd.tree(def) else { unreachable!() };
        assert_eq!(tpd.clause_items(changed)[0], first);
        assert_eq!(tpd.children(def), [p, new, tpt]);
        assert_eq!(tpd.clause_items(paramss), [first, second]);
    }
}
