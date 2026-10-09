//! dotty's `ExpandPrivate`: private members used from another class, a nested one or a
//! companion, get expanded names and become public.

use crate::Lower;
use crate::symbols::{has, is_class};
use tasty::tags::PRIVATE;
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, classes: &[TNodeId]) {
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        for t in super::explicit_outer::terms(l, td, false) {
            if !matches!(l.tpd.tree(t), Tree::Ident { .. } | Tree::Select { .. }) {
                continue;
            }
            let Some(s) = l.tpd.sym(t) else { continue };
            let Some(owner) = l.u.symbol(s).owner else { continue };
            if owner != class && is_class(l.u, owner) && has(l.u, s, PRIVATE) && l.u.symbol(s).kind.is_term()
            {
                super::accessor_names::ensure_not_private(l, s);
            }
        }
    }
}
