// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ElimOpaque.scala
//! Opaque types turned into aliases (dotty's `ElimOpaque`), `==` and `!=` on their values
//! resolved again with the underlying types.

use super::walk::subtrees;
use crate::codegen::callee::is_primitive;
use crate::{Lower, Result, unsupported};
use tasty::tags::OPAQUE;
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, root: TNodeId) -> Result<()> {
    let mut equalities = Vec::new();
    for t in subtrees(l.tpd, root) {
        let Tree::Apply { fun, .. } = *l.tpd.tree(t) else { continue };
        let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { continue };
        let any = Some(l.u.builtins().any);
        let equality = l
            .tpd
            .sym(fun)
            .filter(|&s| l.u.symbol(s).owner == any && matches!(l.u.symbol(s).name.as_str(), "==" | "!="));
        if let Some(s) = equality.filter(|_| is_opaque(l, l.tpd.tpe(qualifier))) {
            equalities.push((t, s, qualifier));
        }
    }
    l.u.elide_opaques(true);
    for (t, s, qualifier) in equalities {
        resolve_equality(l, t, s, qualifier)?;
    }
    Ok(())
}

/// Call `t` of `==` or `!=` (`s`) on `qualifier` of an opaque type whose underlying type is a
/// primitive value class made a call of that class's method taking the argument's class (dotty's
/// `applyOverloaded`).
fn resolve_equality(l: &mut Lower, t: TNodeId, s: SymbolId, qualifier: TNodeId) -> Result<()> {
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return Ok(()) };
    let Some(class) = underlying_class(l, qualifier) else { return Ok(()) };
    if !is_primitive(&l.u.full_name(class)) {
        return Ok(());
    }
    let arg = match *l.tpd.items(args) {
        [arg] => underlying_class(l, arg),
        _ => None,
    };
    let name = l.u.symbol(s).name.clone();
    let method = l.u.decl(class, &name).unwrap_or_default().into_iter().find(|&m| {
        let info = l.u.info(m);
        let param = l.u.method(info).and_then(|mt| mt.params.first().map(|&(_, p)| p));
        arg.is_some() && param.is_some_and(|p| l.u.class_symbol(p) == arg)
    });
    let Some(method) = method else { return unsupported(l.tpd.span(t), "equality of opaque type values") };
    let select = l.select(qualifier, method, l.tpd.span(fun));
    l.tpd.get_mut(t).tree = Tree::Apply { fun: select, args };
    Ok(())
}

/// The class of the type of `t`, widened and dealiased.
fn underlying_class(l: &mut Lower, t: TNodeId) -> Option<SymbolId> {
    let widened = l.u.widen(l.tpd.tpe(t));
    let dealiased = l.u.dealias(widened);
    l.u.class_symbol(dealiased)
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
