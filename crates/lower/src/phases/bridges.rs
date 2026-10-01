//! Where dotty's `Bridges` adds a bridge: a method of a class overriding a member with the same
//! erased parameters but another erased result type. Refused until `Bridges` is ported.

use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use tasty::Kind;
use tasty::tags::{FIELDaccessor, PRIVATE};
use typer::tpd::{TNodeId, Tree};

pub fn check(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { body, .. } = *l.tpd.tree(rhs) else { return Ok(()) };
    let bases = l.u.linearization(class);
    for stat in l.tpd.items(body).to_vec() {
        let Some(m) = l.tpd.sym(stat) else { continue };
        let method = l.u.symbol(m).kind == Kind::Def || has(l.u, m, FIELDaccessor);
        if !method || has(l.u, m, PRIVATE) {
            continue;
        }
        let name = l.u.symbol(m).name.clone();
        let (params, result) = l.u.erased_method(m);
        for &base in &bases[1..] {
            for other in l.u.decl(base, &name).unwrap_or_default() {
                let member =
                    matches!(l.u.symbol(other).kind, Kind::Def | Kind::Val) && !has(l.u, other, PRIVATE);
                if member && name != "<init>" {
                    let (other_params, other_result) = l.u.erased_method(other);
                    if other_params == params && other_result != result {
                        return unsupported(l.tpd.span(stat), "overrides that need bridges");
                    }
                }
            }
        }
    }
    Ok(())
}
