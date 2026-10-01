//! dotty's `Bridges` for result types: a method of a class overriding a member with the same
//! erased parameters but another erased result type gets a bridge of the member's type that
//! calls it.

use crate::symbols::{has, is_constructor, is_trait};
use crate::{Lower, Result, unsupported};
use tasty::tags::{FIELDaccessor, PRIVATE};
use tasty::{Erased, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let bases = l.u.linearization(class);
    let mut stats = l.tpd.items(body).to_vec();
    let mut bridged: Vec<(String, Vec<Erased>, Erased)> = Vec::new();
    for stat in l.tpd.items(body).to_vec() {
        let Some(m) = l.tpd.sym(stat) else { continue };
        let method = l.u.symbol(m).kind == Kind::Def || has(l.u, m, FIELDaccessor);
        if !method || has(l.u, m, PRIVATE) || is_constructor(l.u, m) {
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
    let (sym, def) = l.method_like(class, m, info, span, |l, args, _| {
        let this = l.this(class, span);
        let fun = l.select(this, m, span);
        l.apply(fun, args, m_result, span)
    })?;
    l.bridges.insert(sym);
    Ok(def)
}
