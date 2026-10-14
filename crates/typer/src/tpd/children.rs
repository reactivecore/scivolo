//! The subtrees of typed trees, and trees rebuilt with mapped subtrees (dotty's `TreeMap`).

use super::slots::Slot;
use super::{Clauses, List, TNodeId, Tpd};
use std::ops::Deref;

const INLINE: usize = 8;

/// The direct subtrees of a tree, held inline up to eight.
#[derive(Clone, Debug)]
pub struct Children {
    inline: [TNodeId; INLINE],
    len: usize,
    spilled: Vec<TNodeId>,
}

impl Children {
    fn push(&mut self, c: TNodeId) {
        if self.len < INLINE {
            self.inline[self.len] = c;
        } else {
            if self.len == INLINE {
                self.spilled.extend_from_slice(&self.inline);
            }
            self.spilled.push(c);
        }
        self.len += 1;
    }
}

impl Deref for Children {
    type Target = [TNodeId];

    fn deref(&self) -> &[TNodeId] {
        if self.len <= INLINE { &self.inline[..self.len] } else { &self.spilled }
    }
}

impl Extend<TNodeId> for Children {
    fn extend<I: IntoIterator<Item = TNodeId>>(&mut self, items: I) {
        items.into_iter().for_each(|c| self.push(c));
    }
}

impl IntoIterator for Children {
    type Item = TNodeId;
    type IntoIter = IntoIter;

    fn into_iter(self) -> IntoIter {
        IntoIter { back: self.len, front: 0, children: self }
    }
}

/// The trees of a `Children`, by value.
pub struct IntoIter {
    children: Children,
    front: usize,
    back: usize,
}

impl Iterator for IntoIter {
    type Item = TNodeId;

    fn next(&mut self) -> Option<TNodeId> {
        (self.front < self.back).then(|| {
            self.front += 1;
            self.children[self.front - 1]
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.back - self.front, Some(self.back - self.front))
    }
}

impl DoubleEndedIterator for IntoIter {
    fn next_back(&mut self) -> Option<TNodeId> {
        (self.front < self.back).then(|| {
            self.back -= 1;
            self.children[self.back]
        })
    }
}

impl ExactSizeIterator for IntoIter {}

impl Tpd {
    /// The direct subtrees of `t`, in order.
    pub fn children(&self, t: TNodeId) -> Children {
        let mut out = Children { inline: [t; INLINE], len: 0, spilled: Vec::new() };
        for &slot in self.tree(t).slots().iter() {
            match slot {
                Slot::Node(c) => out.push(c),
                Slot::Opt(c) => out.extend(c),
                Slot::List(l) => out.extend(self.items(l).iter().copied()),
                Slot::Clauses(cs) => {
                    self.clause_items(cs).iter().for_each(|&l| out.extend(self.items(l).iter().copied()))
                }
            }
        }
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
    let tree = cx.as_mut().tree(t);
    let mut slots = tree.slots();
    if slots.is_empty() {
        return false;
    }
    let mut tree = tree.clone();
    let mut changed = false;
    for slot in slots.iter_mut() {
        changed |= match slot {
            Slot::Node(c) => map_one(cx, c, f),
            Slot::Opt(c) => c.as_mut().is_some_and(|c| map_one(cx, c, f)),
            Slot::List(l) => map_list(cx, l, f),
            Slot::Clauses(cs) => map_clauses(cx, cs, f),
        };
    }
    if changed {
        tree.set_slots(&slots);
        cx.as_mut().get_mut(t).tree = tree;
    }
    changed
}

fn map_one<C: AsMut<Tpd> + ?Sized>(cx: &mut C, c: &mut TNodeId, f: &mut MapChild<C>) -> bool {
    let new = f(cx, *c);
    std::mem::replace(c, new) != new
}

fn map_list<C: AsMut<Tpd> + ?Sized>(cx: &mut C, l: &mut List, f: &mut MapChild<C>) -> bool {
    let old = *l;
    let Some(items) = mapped(cx, old.len(), |tpd, i| tpd.items(old)[i], |cx, c| f(cx, c)) else {
        return false;
    };
    *l = cx.as_mut().list(&items);
    true
}

fn map_clauses<C: AsMut<Tpd> + ?Sized>(cx: &mut C, cs: &mut Clauses, f: &mut MapChild<C>) -> bool {
    let old = *cs;
    let map = |cx: &mut C, mut l: List| {
        map_list(cx, &mut l, f);
        l
    };
    let Some(lists) = mapped(cx, old.len(), |tpd, i| tpd.clause_items(old)[i], map) else { return false };
    *cs = cx.as_mut().clauses(&lists);
    true
}

/// The `len` items that `item` reads, each mapped by `f`, if one of them changed.
fn mapped<C: AsMut<Tpd> + ?Sized, T: Copy + PartialEq>(
    cx: &mut C,
    len: usize,
    item: impl Fn(&Tpd, usize) -> T,
    mut f: impl FnMut(&mut C, T) -> T,
) -> Option<Vec<T>> {
    let mut out: Option<Vec<T>> = None;
    for i in 0..len {
        let old = item(cx.as_mut(), i);
        let new = f(cx, old);
        match &mut out {
            Some(out) => out.push(new),
            None if new != old => {
                let mut changed: Vec<T> = (0..i).map(|j| item(cx.as_mut(), j)).collect();
                changed.push(new);
                out = Some(changed);
            }
            None => {}
        }
    }
    out
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
        assert_eq!(*tpd.children(block), [c, a, a]);
    }

    #[test]
    fn maps_present_optional_children_only() {
        let mut tpd = Tpd::default();
        let (tpt, rhs, new) = (leaf(&mut tpd), leaf(&mut tpd), leaf(&mut tpd));
        let name = name();
        let none = tpd.node(Tree::ValDef { name, tpt, rhs: None }, Span::NONE, None, None);
        let some = tpd.node(Tree::ValDef { name, tpt, rhs: Some(rhs) }, Span::NONE, None, None);
        assert!(!swap(&mut tpd, none, rhs, new));
        assert_eq!(*tpd.children(none), [tpt]);
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
        assert_eq!(*tpd.children(def), [p, q, tpt]);
        assert!(!swap(&mut tpd, def, new, p));
        assert!(swap(&mut tpd, def, q, new));
        let Tree::DefDef { paramss: changed, .. } = *tpd.tree(def) else { unreachable!() };
        assert_eq!(tpd.clause_items(changed)[0], first);
        assert_eq!(*tpd.children(def), [p, new, tpt]);
        assert_eq!(tpd.clause_items(paramss), [first, second]);
    }
}
