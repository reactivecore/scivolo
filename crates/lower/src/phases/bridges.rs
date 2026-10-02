//! dotty's `Bridges` for result types: a concrete method of a class overriding a member with the
//! same erased parameters but another erased result type gets a bridge of the member's type
//! that calls it.

use crate::symbols::{has, is_class, is_constructor, is_trait};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::tags::{CASEaccessor, FIELDaccessor, LAZY, OBJECT, PARAMsetter, PRIVATE};
use tasty::{Erased, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId, deferred: &FxHashSet<SymbolId>) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let span = l.tpd.span(td);
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let bases = l.u.linearization(class);
    check_inherited(l, class, &bases, deferred, span)?;
    let mut stats = l.tpd.items(body).to_vec();
    let mut bridged: Vec<(String, Vec<Erased>, Erased)> = Vec::new();
    for stat in l.tpd.items(body).to_vec() {
        let (Tree::DefDef { rhs: Some(_), .. }, Some(m)) = (l.tpd.tree(stat), l.tpd.sym(stat)) else {
            continue;
        };
        if !is_member(l, m) {
            continue;
        }
        let name = l.u.symbol(m).name.clone();
        let (params, result) = l.u.erased_method(m);
        for &base in &bases[1..] {
            for other in l.u.decl(base, &name).unwrap_or_default() {
                if !is_member(l, other) {
                    continue;
                }
                let (other_params, other_result) = l.u.erased_method(other);
                let signature = (name.clone(), other_params.clone(), other_result.clone());
                if other_params != params || other_result == result || bridged.contains(&signature) {
                    continue;
                }
                if is_trait(l.u, class) {
                    return unsupported(l.tpd.span(stat), "bridges in traits");
                }
                bridged.push(signature);
                stats.push(bridge(l, class, m, other, l.tpd.span(stat))?);
            }
        }
    }
    if !bridged.is_empty() {
        let body = l.tpd.list(&stats);
        l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    }
    Ok(())
}

/// Whether `s` is a non-private method or value a class can override.
fn is_member(l: &Lower, s: SymbolId) -> bool {
    matches!(l.u.symbol(s).kind, Kind::Def | Kind::Val) && !has(l.u, s, PRIVATE) && !is_constructor(l.u, s)
}

/// Fails if a member of a trait `class` mixes in is implemented by an inherited member of
/// another erased result type, which needs a bridge `Bridges` makes from inherited pairs.
fn check_inherited(
    l: &mut Lower,
    class: SymbolId,
    bases: &[SymbolId],
    deferred: &FxHashSet<SymbolId>,
    span: syntax::ast::Span,
) -> Result<()> {
    let superclass = bases[1..].iter().copied().find(|&b| is_class(l.u, b) && !is_trait(l.u, b));
    let inherited = superclass.map(|s| l.u.linearization(s)).unwrap_or_default();
    for &base in bases[1..].iter().filter(|b| !inherited.contains(b)) {
        let decls: Vec<SymbolId> = l.u.decls(base).unwrap_or_default();
        let members: Vec<SymbolId> = decls.into_iter().filter(|&d| is_member(l, d)).collect();
        for d in members {
            let name = l.u.symbol(d).name.clone();
            let (params, result) = l.u.erased_method(d);
            let implementation = bases.iter().find_map(|&b| {
                let found = l.u.decl(b, &name).unwrap_or_default();
                found
                    .into_iter()
                    .find(|&o| is_member(l, o) && !deferred.contains(&o) && l.u.erased_method(o).0 == params)
            });
            let Some(implementation) = implementation else { continue };
            let inherited_impl = l.u.symbol(implementation).owner != Some(class);
            if inherited_impl && l.u.erased_method(implementation).1 != result {
                return unsupported(span, "bridges for inherited members");
            }
        }
    }
    Ok(())
}

/// `def m(params): R = this.m(params)` with the type of `other` that `m` overrides.
fn bridge(
    l: &mut Lower,
    class: SymbolId,
    m: SymbolId,
    other: SymbolId,
    span: syntax::ast::Span,
) -> Result<TNodeId> {
    let info = l.u.info(other);
    let m_info = l.u.info(m);
    let m_result = l.u.result_type(m_info);
    let mut flags = l.u.symbol(m).flags;
    for tag in [FIELDaccessor, PARAMsetter, CASEaccessor, LAZY, OBJECT] {
        flags = flags.without(tag);
    }
    let (sym, def) = l.method_like(class, m, flags, info, span, |l, args, _| {
        let this = l.this(class, span);
        let fun = l.select(this, m, span);
        l.apply(fun, args, m_result, span)
    })?;
    l.bridges.insert(sym);
    Ok(def)
}
