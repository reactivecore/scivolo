//! The subtrees of typed trees, and the symbols trees refer to.

use crate::Lower;
use rustc_hash::FxHashSet;
use syntax::ast::constant::Constant;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tpd, Tree};

impl AsRef<Tpd> for Lower<'_> {
    fn as_ref(&self) -> &Tpd {
        self.tpd
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
