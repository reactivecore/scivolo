//! The walks of the phases over a class definition, the subtrees of typed trees, and the
//! symbols trees refer to.

use crate::{Lower, Result};
use rustc_hash::FxHashSet;
use syntax::ast::constant::Constant;
use tasty::SymbolId;
use typer::tpd::{self, Leave, TNodeId, Tpd, Tree, Visit, Walk, traverse, traverse_up};

impl AsRef<Tpd> for Lower<'_> {
    fn as_ref(&self) -> &Tpd {
        self.tpd
    }
}

impl AsMut<Tpd> for Lower<'_> {
    fn as_mut(&mut self) -> &mut Tpd {
        self.tpd
    }
}

/// `typer::tpd::map_children` with a mapping that can fail; the first error ends the mapping.
pub fn map_children<'a>(
    l: &mut Lower<'a>,
    t: TNodeId,
    f: &mut dyn FnMut(&mut Lower<'a>, TNodeId) -> Result<TNodeId>,
) -> Result<()> {
    let mut failed = None;
    tpd::map_children(l, t, &mut |l, c| match failed {
        Some(_) => c,
        None => f(l, c).unwrap_or_else(|e| {
            failed = Some(e);
            c
        }),
    });
    failed.map_or(Ok(()), Err)
}

/// `typer::tpd::traverse` from class definition `td`, owned by its class, that does not enter
/// the type and class definitions nested in it.
pub fn walk_class<'a>(l: &mut Lower<'a>, td: TNodeId, enter: &mut Visit<Lower<'a>>) {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    traverse(l, td, class, &mut outside_nested(td, enter));
}

/// `walk_class` as `typer::tpd::traverse_up`, also calling `leave` on each tree entered, the
/// nested definitions skipped too.
pub fn walk_class_up<'a>(
    l: &mut Lower<'a>,
    td: TNodeId,
    enter: &mut Visit<Lower<'a>>,
    leave: &mut Leave<Lower<'a>>,
) {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    traverse_up(l, td, class, &mut outside_nested(td, enter), leave);
}

/// `walk_class` of a fallible `enter`, ended by its first error.
pub fn try_walk_class<'a>(
    l: &mut Lower<'a>,
    td: TNodeId,
    enter: &mut dyn FnMut(&mut Lower<'a>, TNodeId, SymbolId) -> Result<Walk>,
) -> Result<()> {
    let mut result = Ok(());
    walk_class(l, td, &mut |l, t, owner| {
        enter(l, t, owner).unwrap_or_else(|e| {
            result = Err(e);
            Walk::Stop
        })
    });
    result
}

/// `enter`, skipping the type and class definitions nested in class definition `td`.
fn outside_nested<'a, 'f>(
    td: TNodeId,
    enter: &'f mut Visit<'_, Lower<'a>>,
) -> impl FnMut(&mut Lower<'a>, TNodeId, SymbolId) -> Walk + 'f {
    move |l, t, owner| {
        if t != td && matches!(l.tpd.tree(t), Tree::TypeDef { .. }) { Walk::Skip } else { enter(l, t, owner) }
    }
}

/// The direct subtrees of `t`, in order.
pub fn children(tpd: &Tpd, t: TNodeId) -> Vec<TNodeId> {
    tpd.children(t)
}

/// `t` and the trees in it, outer trees first.
pub fn subtrees(tpd: &Tpd, t: TNodeId) -> Vec<TNodeId> {
    let mut out = vec![t];
    let mut i = 0;
    while i < out.len() {
        out.extend(children(tpd, out[i]));
        i += 1;
    }
    out
}

/// Whether `a` and `b` are the same identifiers, selections, literals, applications or type
/// applications (dotty's `===`).
pub fn same(tpd: &Tpd, a: TNodeId, b: TNodeId) -> bool {
    match (tpd.tree(a), tpd.tree(b)) {
        (Tree::Ident { .. }, Tree::Ident { .. }) => tpd.sym(a) == tpd.sym(b),
        (&Tree::Select { qualifier: q1, .. }, &Tree::Select { qualifier: q2, .. }) => {
            tpd.sym(a) == tpd.sym(b) && same(tpd, q1, q2)
        }
        (Tree::Literal { value: c1 }, Tree::Literal { value: c2 }) => match (c1, c2) {
            (Constant::Double(x), Constant::Double(y)) => x.to_bits() == y.to_bits(),
            (Constant::Float(x), Constant::Float(y)) => x.to_bits() == y.to_bits(),
            _ => c1 == c2,
        },
        (&Tree::Apply { fun: f1, args: a1 }, &Tree::Apply { fun: f2, args: a2 }) => {
            let (a1, a2) = (tpd.items(a1), tpd.items(a2));
            same(tpd, f1, f2) && a1.len() == a2.len() && a1.iter().zip(a2).all(|(&x, &y)| same(tpd, x, y))
        }
        (&Tree::TypeApply { fun: f1, args: a1 }, &Tree::TypeApply { fun: f2, args: a2 }) => {
            let (a1, a2) = (tpd.items(a1), tpd.items(a2));
            same(tpd, f1, f2)
                && a1.len() == a2.len()
                && a1.iter().zip(a2).all(|(&x, &y)| tpd.tpe(x) == tpd.tpe(y))
        }
        _ => false,
    }
}

/// The symbols that the identifiers and selections in `t` refer to.
pub fn references(tpd: &Tpd, t: TNodeId) -> FxHashSet<SymbolId> {
    tpd.fold(FxHashSet::default(), t, &mut |mut out, t| {
        if matches!(tpd.tree(t), Tree::Ident { .. } | Tree::Select { .. }) {
            out.extend(tpd.sym(t));
        }
        out
    })
}
