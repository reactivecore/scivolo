//! Selections on a package of members of its package object: `p.f` becomes `p.package.f`, the
//! tree dotty's typer gives them.

use crate::Lower;
use crate::symbols::is_package_object;
use tasty::Kind;
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) {
    for t in super::explicit_outer::terms(l, td, false) {
        let Tree::Select { qualifier, name } = *l.tpd.tree(t) else { continue };
        let package = l.tpd.sym(qualifier).is_some_and(|s| l.u.symbol(s).kind == Kind::Package);
        let Some(owner) = l.tpd.sym(t).and_then(|s| l.u.symbol(s).owner) else { continue };
        if !package || !is_package_object(l.u, owner) {
            continue;
        }
        let Some(module) = l.u.source_module(owner) else { continue };
        let span = l.tpd.span(qualifier);
        let qualifier = l.ident(module, span);
        l.tpd.get_mut(t).tree = Tree::Select { qualifier, name };
    }
}
