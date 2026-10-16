//! dotty's `TreeTraverser` and `TreeAccumulator` over typed trees: children in the order of
//! `Tpd::children`, each with the owner dotty's `localCtx` gives it and, for a template, the
//! owners of `MegaPhase` and `TreeMap.transformStats` (`superCallContext` for the parents,
//! `mapStatements` for the statements), which dotty's `TreeAccumulator` does not apply.

use super::{TNodeId, Tpd, Tree};
use std::ops::ControlFlow;
use tasty::SymbolId;

/// What a walk does after entering a tree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Walk {
    /// Enters the children, with the owners `Tpd::owned_children` gives them.
    Children,
    /// Enters the children, owned by the given symbol.
    Within(SymbolId),
    /// Leaves the tree without entering its children.
    Skip,
    /// Ends the walk.
    Stop,
}

/// A walk's callback on entering a tree, called with the context, the tree and the tree's owner.
pub type Visit<'f, C> = dyn FnMut(&mut C, TNodeId, SymbolId) -> Walk + 'f;

/// A walk's callback on leaving a tree, as `Visit`; `Break` ends the walk.
pub type Leave<'f, C> = dyn FnMut(&mut C, TNodeId, SymbolId) -> ControlFlow<()> + 'f;

impl AsRef<Tpd> for Tpd {
    fn as_ref(&self) -> &Tpd {
        self
    }
}

/// Calls `enter` on `t`, owned by `owner`, and then, as it says, on the trees under `t`, outer
/// trees first; `enter` may rewrite the tree it is called on in place, and the walk then enters
/// the children of the new tree.
pub fn traverse<C: AsRef<Tpd> + ?Sized>(cx: &mut C, t: TNodeId, owner: SymbolId, enter: &mut Visit<C>) {
    walk(cx, t, owner, enter, None);
}

/// `traverse`, also calling `leave` on each tree entered, with the tree's owner, once the walk is
/// done with its children; for a tree `enter` skipped, right after `enter`.
pub fn traverse_up<C: AsRef<Tpd> + ?Sized>(
    cx: &mut C,
    t: TNodeId,
    owner: SymbolId,
    enter: &mut Visit<C>,
    leave: &mut Leave<C>,
) {
    walk(cx, t, owner, enter, Some(leave));
}

enum Step {
    Enter(TNodeId, SymbolId),
    Leave(TNodeId, SymbolId),
}

fn walk<C: AsRef<Tpd> + ?Sized>(
    cx: &mut C,
    t: TNodeId,
    owner: SymbolId,
    enter: &mut Visit<C>,
    mut leave: Option<&mut Leave<C>>,
) {
    let mut todo = vec![Step::Enter(t, owner)];
    while let Some(step) = todo.pop() {
        match step {
            Step::Enter(t, owner) => {
                let walk = enter(cx, t, owner);
                if walk == Walk::Stop {
                    return;
                }
                if leave.is_some() {
                    todo.push(Step::Leave(t, owner));
                }
                let tpd = cx.as_ref();
                let enter = |(c, owner)| Step::Enter(c, owner);
                match walk {
                    Walk::Children => todo.extend(tpd.owned_children(t, owner).rev().map(enter)),
                    Walk::Within(inner) => {
                        todo.extend(tpd.children(t).into_iter().rev().map(|c| enter((c, inner))))
                    }
                    _ => {}
                }
            }
            Step::Leave(t, owner) => {
                if leave.as_mut().is_some_and(|leave| leave(cx, t, owner).is_break()) {
                    return;
                }
            }
        }
    }
}

impl Tpd {
    /// The children of `t`, a tree owned by `owner`, each with its owner: `t`'s symbol if `t` is
    /// a definition that has one; for a template, the primary constructor for the parents and
    /// the template's symbol for the statements that are not definitions or imports; else
    /// `owner`.
    pub fn owned_children(
        &self,
        t: TNodeId,
        owner: SymbolId,
    ) -> impl DoubleEndedIterator<Item = (TNodeId, SymbolId)> + '_ {
        let children = self.children(t).into_iter();
        let (mut inner, mut by_inner, mut stats, mut dummy) = (owner, 0..0, usize::MAX, owner);
        match *self.tree(t) {
            Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::PackageDef { .. } => {
                (inner, by_inner) = (self.sym(t).unwrap_or(owner), 0..usize::MAX);
            }
            Tree::Template { constr, parents, body, .. } => {
                (inner, by_inner) = (self.sym(constr).unwrap_or(owner), 1..self.items(parents).len() + 1);
                dummy = self.sym(t).unwrap_or(owner);
                stats = children.len() - self.items(body).len();
            }
            _ => {}
        }
        let is_def = |c| {
            matches!(
                self.tree(c),
                Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. }
            )
        };
        children.enumerate().map(move |(i, c)| match i {
            _ if by_inner.contains(&i) => (c, inner),
            _ if i >= stats && !is_def(c) => (c, dummy),
            _ => (c, owner),
        })
    }

    /// `f` folded over `t` and the trees under it, outer trees first (dotty's `DeepFolder`).
    pub fn fold<X>(&self, x: X, t: TNodeId, f: &mut dyn FnMut(X, TNodeId) -> X) -> X {
        let (mut x, mut todo) = (x, vec![t]);
        while let Some(t) = todo.pop() {
            x = f(x, t);
            todo.extend(self.children(t).into_iter().rev());
        }
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syntax::ast::{Span, TName};
    use syntax::names::Names;

    fn name() -> TName {
        TName { name: Names::default().intern("x"), is_type: false }
    }

    fn node(tpd: &mut Tpd, tree: Tree, sym: Option<u32>) -> TNodeId {
        tpd.node(tree, Span::NONE, None, sym.map(SymbolId))
    }

    /// `{ val v = a; def m = b }` as a block of class `C` (symbol 1), `v` symbol 2, `m` symbol 3,
    /// with the trees in pre-order.
    fn unit() -> (Tpd, Vec<TNodeId>) {
        let name = name();
        let mut tpd = Tpd::default();
        let tpt = node(&mut tpd, Tree::TypeTree { inferred: true }, None);
        let a = node(&mut tpd, Tree::Ident { name }, None);
        let v = node(&mut tpd, Tree::ValDef { name, tpt, rhs: Some(a) }, Some(2));
        let tpt2 = node(&mut tpd, Tree::TypeTree { inferred: true }, None);
        let b = node(&mut tpd, Tree::Ident { name }, None);
        let paramss = tpd.clauses(&[]);
        let m = node(&mut tpd, Tree::DefDef { name, paramss, tpt: tpt2, rhs: Some(b) }, Some(3));
        let stats = tpd.list(&[v]);
        let block = node(&mut tpd, Tree::Block { stats, expr: m }, None);
        (tpd, vec![block, v, tpt, a, m, tpt2, b])
    }

    /// The owner each tree under `root` is entered with, from owner 1.
    fn owners(tpd: &mut Tpd, root: TNodeId) -> Vec<(TNodeId, u32)> {
        let mut seen = Vec::new();
        traverse(tpd, root, SymbolId(1), &mut |_, t, owner| {
            seen.push((t, owner.0));
            Walk::Children
        });
        seen
    }

    #[test]
    fn enters_outer_trees_first_with_their_owners() {
        let (mut tpd, order) = unit();
        let owners = owners(&mut tpd, order[0]);
        assert_eq!(owners, order.iter().copied().zip([1, 1, 2, 2, 1, 3, 3]).collect::<Vec<_>>());
    }

    #[test]
    fn type_definitions_own_their_children_and_definitions_without_symbols_do_not() {
        let name = name();
        let mut tpd = Tpd::default();
        let a = node(&mut tpd, Tree::Ident { name }, None);
        let class = node(&mut tpd, Tree::TypeDef { name, rhs: a }, Some(5));
        let tpt = node(&mut tpd, Tree::TypeTree { inferred: true }, None);
        let b = node(&mut tpd, Tree::Ident { name }, None);
        let anonymous = node(&mut tpd, Tree::ValDef { name, tpt, rhs: Some(b) }, None);
        assert_eq!(owners(&mut tpd, class), [(class, 1), (a, 5)]);
        assert_eq!(owners(&mut tpd, anonymous), [(anonymous, 1), (tpt, 1), (b, 1)]);
    }

    #[test]
    fn template_statements_but_definitions_are_owned_by_the_local_dummy() {
        let name = name();
        let mut tpd = Tpd::default();
        let (paramss, tpt) = (tpd.clauses(&[]), node(&mut tpd, Tree::TypeTree { inferred: true }, None));
        let constr = node(&mut tpd, Tree::DefDef { name, paramss, tpt, rhs: None }, Some(6));
        let parent = node(&mut tpd, Tree::Ident { name }, None);
        let tpt2 = node(&mut tpd, Tree::TypeTree { inferred: true }, None);
        let v = node(&mut tpd, Tree::ValDef { name, tpt: tpt2, rhs: None }, Some(7));
        let expr = node(&mut tpd, Tree::Ident { name }, None);
        let selectors = tpd.list(&[]);
        let import = node(&mut tpd, Tree::Import { expr, selectors, export: false }, None);
        let e = node(&mut tpd, Tree::Ident { name }, None);
        let (parents, body) = (tpd.list(&[parent]), tpd.list(&[v, import, e]));
        let template = Tree::Template { constr, parents, self_: None, body };
        let template = node(&mut tpd, template, Some(8));
        let class = node(&mut tpd, Tree::TypeDef { name, rhs: template }, Some(5));
        let expected = [
            (class, 1),
            (template, 5),
            (constr, 5),
            (tpt, 6),
            (parent, 6),
            (v, 5),
            (tpt2, 7),
            (import, 5),
            (expr, 5),
            (e, 8),
        ];
        assert_eq!(owners(&mut tpd, class), expected);
    }

    #[test]
    fn a_template_of_the_class_symbol_gives_its_definitions_the_class() {
        let name = name();
        let mut tpd = Tpd::default();
        let (paramss, tpt) = (tpd.clauses(&[]), node(&mut tpd, Tree::TypeTree { inferred: true }, None));
        let constr = node(&mut tpd, Tree::DefDef { name, paramss, tpt, rhs: None }, Some(6));
        let tpt2 = node(&mut tpd, Tree::TypeTree { inferred: true }, None);
        let v = node(&mut tpd, Tree::ValDef { name, tpt: tpt2, rhs: None }, Some(7));
        let (parents, body) = (tpd.list(&[]), tpd.list(&[v]));
        let template = Tree::Template { constr, parents, self_: None, body };
        let template = node(&mut tpd, template, Some(5));
        let class = node(&mut tpd, Tree::TypeDef { name, rhs: template }, Some(5));
        let expected = [(class, 1), (template, 5), (constr, 5), (tpt, 6), (v, 5), (tpt2, 7)];
        assert_eq!(owners(&mut tpd, class), expected);
    }

    #[test]
    fn leaves_inner_trees_first_and_skipped_trees_too() {
        let (mut tpd, order) = unit();
        let [block, v, tpt, a, m, tpt2, b] = order[..] else { unreachable!() };
        let mut seen = Vec::new();
        let mut enter = |_: &mut Tpd, _, _| Walk::Children;
        traverse_up(&mut tpd, block, SymbolId(1), &mut enter, &mut |_, t, _| {
            seen.push(t);
            ControlFlow::Continue(())
        });
        assert_eq!(seen, [tpt, a, v, tpt2, b, m, block]);
        seen.clear();
        let mut enter = |_: &mut Tpd, t, _| if t == v { Walk::Skip } else { Walk::Children };
        traverse_up(&mut tpd, block, SymbolId(1), &mut enter, &mut |_, t, _| {
            seen.push(t);
            ControlFlow::Continue(())
        });
        assert_eq!(seen, [v, tpt2, b, m, block]);
    }

    #[test]
    fn a_break_from_leave_ends_the_walk_and_leave_gets_the_owner() {
        let (mut tpd, order) = unit();
        let [block, _, tpt, a, ..] = order[..] else { unreachable!() };
        let mut seen = Vec::new();
        let mut entered = 0;
        let mut enter = |_: &mut Tpd, _, _| {
            entered += 1;
            Walk::Children
        };
        traverse_up(&mut tpd, block, SymbolId(1), &mut enter, &mut |_, t, owner| {
            seen.push((t, owner.0));
            if t == a { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
        });
        assert_eq!(seen, [(tpt, 2), (a, 2)]);
        assert_eq!(entered, 4);
    }

    #[test]
    fn skips_stops_and_changes_owners() {
        let (mut tpd, order) = unit();
        let (v, m) = (order[1], order[4]);
        let mut seen = Vec::new();
        traverse(&mut tpd, order[0], SymbolId(1), &mut |_, t, owner| {
            seen.push((t, owner.0));
            match t {
                _ if t == v => Walk::Skip,
                _ if t == m => Walk::Within(SymbolId(9)),
                _ if owner.0 == 9 => Walk::Stop,
                _ => Walk::Children,
            }
        });
        assert_eq!(seen, [(order[0], 1), (v, 1), (m, 1), (order[5], 9)]);
    }

    #[test]
    fn enters_the_children_of_a_tree_rewritten_on_entering() {
        let (mut tpd, order) = unit();
        let a = order[3];
        let name = name();
        let mut made = Vec::new();
        let mut seen = Vec::new();
        traverse(&mut tpd, order[0], SymbolId(1), &mut |tpd, t, _| {
            seen.push(t);
            if t == a {
                let expr = node(tpd, Tree::Ident { name }, None);
                let tpt = node(tpd, Tree::TypeTree { inferred: true }, None);
                tpd.get_mut(t).tree = Tree::Typed { expr, tpt };
                made = vec![expr, tpt];
            }
            Walk::Children
        });
        let at = seen.iter().position(|&t| t == a).expect("a entered");
        assert_eq!(seen[at + 1..at + 3], made[..]);
    }

    #[test]
    fn folds_in_pre_order() {
        let (tpd, order) = unit();
        let all = tpd.fold(Vec::new(), order[0], &mut |mut out, t| {
            out.push(t);
            out
        });
        assert_eq!(all, order);
    }
}
