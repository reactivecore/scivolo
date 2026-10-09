//! dotty's `TreeTraverser` and `TreeAccumulator` over typed trees: children in the order of
//! `Tpd::children`, each with the owner dotty's `localCtx` gives it.

use super::{TNodeId, Tpd, Tree};
use tasty::SymbolId;

/// What a walk does after entering a tree.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Walk {
    /// Enters the children, owned by the tree's symbol if it is a definition (`Tpd::local_owner`).
    Children,
    /// Enters the children, owned by the given symbol.
    Within(SymbolId),
    /// Leaves the tree without entering its children.
    Skip,
    /// Ends the walk.
    Stop,
}

/// A walk's callback, called with the context, the tree and the tree's owner.
pub type Visit<'f, C> = dyn FnMut(&mut C, TNodeId, SymbolId) -> Walk + 'f;

impl AsRef<Tpd> for Tpd {
    fn as_ref(&self) -> &Tpd {
        self
    }
}

/// Calls `enter` on `t`, owned by `owner`, and then, as it says, on the trees under `t`, outer
/// trees first; `enter` may rewrite the tree it is called on in place.
pub fn traverse<C: AsRef<Tpd> + ?Sized>(cx: &mut C, t: TNodeId, owner: SymbolId, enter: &mut Visit<C>) {
    walk(cx, t, owner, enter, None);
}

/// `traverse`, also calling `leave` on each tree entered once the walk is done with its children,
/// with the tree's owner; `Walk::Stop` from `leave` ends the walk, its other answers do nothing.
pub fn traverse_up<C: AsRef<Tpd> + ?Sized>(
    cx: &mut C,
    t: TNodeId,
    owner: SymbolId,
    enter: &mut Visit<C>,
    leave: &mut Visit<C>,
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
    mut leave: Option<&mut Visit<C>>,
) {
    let mut todo = vec![Step::Enter(t, owner)];
    while let Some(step) = todo.pop() {
        match step {
            Step::Enter(t, owner) => {
                let inner = match enter(cx, t, owner) {
                    Walk::Stop => return,
                    Walk::Skip => None,
                    Walk::Children => Some(cx.as_ref().local_owner(t, owner)),
                    Walk::Within(inner) => Some(inner),
                };
                if leave.is_some() {
                    todo.push(Step::Leave(t, owner));
                }
                if let Some(inner) = inner {
                    let children = cx.as_ref().children(t);
                    todo.extend(children.into_iter().rev().map(|c| Step::Enter(c, inner)));
                }
            }
            Step::Leave(t, owner) => {
                if leave.as_mut().is_some_and(|leave| leave(cx, t, owner) == Walk::Stop) {
                    return;
                }
            }
        }
    }
}

impl Tpd {
    /// The owner of the children of `t`, a tree owned by `owner` (dotty's `localCtx`): `t`'s
    /// symbol if `t` defines one, else `owner`.
    pub fn local_owner(&self, t: TNodeId, owner: SymbolId) -> SymbolId {
        match self.tree(t) {
            Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::PackageDef { .. } => {
                self.sym(t).unwrap_or(owner)
            }
            _ => owner,
        }
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

    /// `{ val v = a; def m = b }` as a block of class `C` (symbol 1), `v` symbol 2, `m` symbol 3,
    /// with the trees in pre-order.
    fn unit() -> (Tpd, Vec<TNodeId>) {
        let mut names = Names::default();
        let name = TName { name: names.intern("x"), is_type: false };
        let mut tpd = Tpd::default();
        let node =
            |tpd: &mut Tpd, tree: Tree, sym: Option<u32>| tpd.node(tree, Span::NONE, None, sym.map(SymbolId));
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

    #[test]
    fn enters_outer_trees_first_with_their_owners() {
        let (mut tpd, order) = unit();
        let mut seen = Vec::new();
        traverse(&mut tpd, order[0], SymbolId(1), &mut |_, t, owner| {
            seen.push((t, owner.0));
            Walk::Children
        });
        let owners = [1, 1, 2, 2, 1, 3, 3];
        assert_eq!(seen, order.iter().copied().zip(owners).collect::<Vec<_>>());
    }

    #[test]
    fn leaves_inner_trees_first() {
        let (mut tpd, order) = unit();
        let mut seen = Vec::new();
        traverse_up(&mut tpd, order[0], SymbolId(1), &mut |_, _, _| Walk::Children, &mut |_, t, _| {
            seen.push(t);
            Walk::Children
        });
        let [block, v, tpt, a, m, tpt2, b] = order[..] else { unreachable!() };
        assert_eq!(seen, [tpt, a, v, tpt2, b, m, block]);
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
    fn folds_in_pre_order() {
        let (tpd, order) = unit();
        let all = tpd.fold(Vec::new(), order[0], &mut |mut out, t| {
            out.push(t);
            out
        });
        assert_eq!(all, order);
    }
}
