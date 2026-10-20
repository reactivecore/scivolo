//! Selections on a package of members of its package object: `p.f` becomes `p.package.f`, the
//! tree dotty's typer gives them; for a member the package object inherits, the package object
//! deriving from the member's class.

use crate::Lower;
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) {
    for t in super::explicit_outer::terms(l, td, false) {
        let Tree::Select { qualifier, name } = *l.tpd.tree(t) else { continue };
        let Some(package) = l.tpd.sym(qualifier).filter(|&s| l.u.symbol(s).kind == Kind::Package) else {
            continue;
        };
        let Some(owner) = l.tpd.sym(t).and_then(|s| l.u.symbol(s).owner) else { continue };
        let module = match l.u.is_package_object(owner) {
            true => l.u.source_module(owner),
            false if l.u.is_class(owner) => inheriting_package_object(l, package, owner),
            false => None,
        };
        let Some(module) = module else { continue };
        let span = l.tpd.span(qualifier);
        let qualifier = l.ident(module, span);
        l.tpd.get_mut(t).tree = Tree::Select { qualifier, name };
    }
}

/// The package object of `package` whose class derives from class `owner`.
fn inheriting_package_object(l: &mut Lower, package: SymbolId, owner: SymbolId) -> Option<SymbolId> {
    let decls = l.u.decls(package).unwrap_or_default();
    decls.into_iter().find(|&d| {
        l.u.is_module(d)
            && l.u.module_class(d).is_some_and(|c| l.u.is_package_object(c) && l.u.derives_from(c, owner))
    })
}
