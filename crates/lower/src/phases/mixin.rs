//! dotty's `Mixin` for traits without initialization: a trait keeps its constructor, which
//! becomes `$init$` without a body; a class gets, for each concrete trait method it inherits, a
//! forwarder calling that method on `super`.

use crate::symbols::{has, is_constructor, is_trait};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::tags::{FIELDaccessor, PRIVATE};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The methods of the unit's classes that have a body if `with_body`, else those without.
pub fn methods_with_body(l: &Lower, classes: &[TNodeId], with_body: bool) -> FxHashSet<SymbolId> {
    let mut out = FxHashSet::default();
    for &td in classes {
        let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { continue };
        let Tree::Template { body, .. } = *l.tpd.tree(rhs) else { continue };
        for &stat in l.tpd.items(body) {
            if let (Tree::DefDef { rhs, .. }, Some(s)) = (l.tpd.tree(stat), l.tpd.sym(stat))
                && rhs.is_some() == with_body
            {
                out.insert(s);
            }
        }
    }
    out
}

pub fn transform(
    l: &mut Lower,
    td: TNodeId,
    unit_classes: &[SymbolId],
    concrete: &FxHashSet<SymbolId>,
) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let span = l.tpd.span(td);
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    if l.tpd.items(body).iter().any(|&s| matches!(l.tpd.tree(s), Tree::TypeDef { .. })) {
        return unsupported(span, "type parameters and members");
    }
    if is_trait(l.u, class) {
        if !l.u.no_inits(class)
            || l.tpd.items(body).iter().any(|&s| !matches!(l.tpd.tree(s), Tree::DefDef { .. }))
        {
            return unsupported(span, "traits with initialization");
        }
        if !matches!(*l.tpd.tree(constr), Tree::DefDef { paramss, .. } if l.tpd.clause_items(paramss).iter().all(|c| c.is_empty()))
        {
            return unsupported(span, "trait parameters");
        }
        return Ok(());
    }
    let mut forwarders = Vec::new();
    for mixin in mixins(l, class) {
        if !unit_classes.contains(&mixin) {
            return unsupported(span, "library traits as parents");
        }
        if !l.u.no_inits(mixin) {
            return unsupported(span, "traits with initialization");
        }
        for meth in l.u.decls(mixin).unwrap_or_default() {
            if concrete.contains(&meth) && needs_forwarder(l, class, meth) {
                forwarders.push(forwarder(l, class, mixin, meth, span)?);
            }
        }
    }
    if forwarders.is_empty() {
        return Ok(());
    }
    let mut stats = l.tpd.items(body).to_vec();
    stats.extend(forwarders);
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// The traits `class` mixes in, from the last one linearized to the first.
fn mixins(l: &mut Lower, class: SymbolId) -> Vec<SymbolId> {
    let linearization = l.u.linearization(class);
    let mut out: Vec<SymbolId> =
        linearization[1..].iter().copied().take_while(|&b| is_trait(l.u, b)).collect();
    out.reverse();
    out
}

/// Whether `meth` is a method `class` inherits as it is: the first non-private method or value
/// of its name and erased parameters in the linearization of `class`.
fn needs_forwarder(l: &mut Lower, class: SymbolId, meth: SymbolId) -> bool {
    let m = l.u.symbol(meth);
    if m.kind != Kind::Def
        || is_constructor(l.u, meth)
        || has(l.u, meth, PRIVATE)
        || has(l.u, meth, FIELDaccessor)
    {
        return false;
    }
    let name = m.name.clone();
    let params = l.u.erased_method(meth).0;
    for base in l.u.linearization(class) {
        for other in l.u.decl(base, &name).unwrap_or_default() {
            let s = l.u.symbol(other);
            let member = matches!(s.kind, Kind::Def | Kind::Val) && !has(l.u, other, PRIVATE);
            if member && l.u.erased_method(other).0 == params {
                return other == meth;
            }
        }
    }
    false
}

/// `def m(params) = C.super[T].m(params)` in class `class` for method `meth` of trait `mixin`.
fn forwarder(
    l: &mut Lower,
    class: SymbolId,
    mixin: SymbolId,
    meth: SymbolId,
    span: syntax::ast::Span,
) -> Result<TNodeId> {
    let info = l.u.info(meth);
    let flags = l.u.symbol(meth).flags;
    let (_, def) = l.method_like(class, meth, flags, info, span, |l, args, result| {
        let sup = l.super_mix(class, mixin, span);
        let fun = l.select(sup, meth, span);
        l.apply(fun, args, result, span)
    })?;
    Ok(def)
}
