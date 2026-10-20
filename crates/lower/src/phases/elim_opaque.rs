// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ElimOpaque.scala
//! Opaque types turned into aliases (dotty's `ElimOpaque`); `==` and `!=` on their values, which
//! dotty resolves again with the underlying types, are refused.

use super::walk::subtrees;
use crate::{Lower, Result, unsupported};
use tasty::Type;
use tasty::tags::OPAQUE;
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, root: TNodeId) -> Result<()> {
    for t in subtrees(l.tpd, root) {
        let Tree::Apply { fun, .. } = *l.tpd.tree(t) else { continue };
        let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { continue };
        let any = Some(l.u.builtins().any);
        let equality = l
            .tpd
            .sym(fun)
            .filter(|&s| l.u.symbol(s).owner == any && matches!(l.u.symbol(s).name.as_str(), "==" | "!="));
        if equality.is_some() && is_opaque(l, l.tpd.tpe(qualifier)) {
            return unsupported(l.tpd.span(t), "equality of opaque type values");
        }
    }
    l.u.elide_opaques(true);
    Ok(())
}

/// Whether `t`, widened and dealiased, refers to an opaque type.
fn is_opaque(l: &mut Lower, t: tasty::TypeId) -> bool {
    let t = l.u.widen(t);
    let t = l.u.dealias(t);
    let s = match *l.u.types().get(t) {
        Type::Sym(_, s) => Some(s),
        Type::TypeRef(..) => l.u.resolve_ref(t),
        _ => None,
    };
    s.is_some_and(|s| l.u.symbol(s).kind == tasty::Kind::Type && l.u.symbol(s).flags.has(OPAQUE))
}
