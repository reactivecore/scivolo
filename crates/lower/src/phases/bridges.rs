//! dotty's `Bridges`: a class, not a trait, gets a bridge for each overriding pair of its
//! members whose erasures differ, of the overridden member's type, calling the overriding one
//! with its arguments cast where their erasures differ.

use super::adapt::adapt;
use super::mixin::{is_jdk, javalib_only_bases, with_java_param_names};
use crate::symbols::{has, is_static_module};
use crate::{Lower, Result};
use rustc_hash::FxHashSet;
use syntax::ast::Span;
use tasty::tags::{CASEaccessor, FIELDaccessor, LAZY, OBJECT, PARAMsetter, PRIVATE};
use tasty::{Erased, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId, deferred: &FxHashSet<SymbolId>) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    if l.u.is_trait(class) {
        return Ok(());
    }
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let mut stats = l.tpd.items(body).to_vec();
    let mut made: Vec<(SymbolId, Vec<Erased>, Erased)> = Vec::new();
    let mut removed = Vec::new();
    for (member, other) in overriding_pairs(l, class, deferred) {
        if is_deferred(l, deferred, member) {
            continue;
        }
        let (params, result) = l.u.erased_method(other);
        if l.u.erased_method(member) == (params.clone(), result.clone()) {
            continue;
        }
        let key = (member, params, result);
        if made.contains(&key) {
            continue;
        }
        made.push(key);
        if l.u.symbol(other).owner == Some(class) {
            removed.push(other);
        }
        let own = stats.iter().copied().find(|&s| l.tpd.sym(s) == Some(member) && l.tpd.span(s).exists());
        let at = Span::at(l.tpd.span(own.unwrap_or(td)).point());
        stats.push(bridge(l, class, member, other, at)?);
    }
    if !made.is_empty() {
        stats.retain(|&s| l.tpd.sym(s).is_none_or(|s| !removed.contains(&s)));
        let body = l.tpd.list(&stats);
        l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    }
    Ok(())
}

/// The pairs of a member of `class` and a member it overrides, in the order dotty's
/// `OverridingPairs.Cursor` yields them for `Bridges`: candidates by name, concrete ones before
/// deferred ones, each from `class` to its last base class but javalib's own in declaration
/// order; a member once
/// overridden overrides nothing; pairs the superclass has too are left out.
fn overriding_pairs(
    l: &mut Lower,
    class: SymbolId,
    deferred: &FxHashSet<SymbolId>,
) -> Vec<(SymbolId, SymbolId)> {
    let hidden = javalib_only_bases(l, class);
    let mut bases = l.u.linearization(class);
    bases.retain(|b| !hidden.contains(b));
    let site = l.u.this_type(class);
    let superclass = bases[1..].iter().copied().find(|&b| l.u.is_class(b) && !l.u.is_trait(b));
    let inherited = superclass.map(|s| l.u.linearization(s)).unwrap_or_default();
    let super_site = superclass.map(|s| l.u.this_type(s));
    let mut candidates = Vec::new();
    for abstract_ in [false, true] {
        for &base in &bases {
            for d in l.u.decls(base).unwrap_or_default() {
                if is_member(l, d) && !l.bridges.contains(&d) && is_deferred(l, deferred, d) == abstract_ {
                    candidates.push(d);
                }
            }
        }
    }
    let mut visited = FxHashSet::default();
    let mut pairs = Vec::new();
    for (i, &overriding) in candidates.iter().enumerate() {
        if visited.contains(&overriding) {
            continue;
        }
        let name = l.u.symbol(overriding).name.clone();
        for &overridden in &candidates[i + 1..] {
            let owners = (l.u.symbol(overriding).owner, l.u.symbol(overridden).owner);
            if l.u.symbol(overridden).name != name || owners.0 == owners.1 {
                continue;
            }
            if !matching(l, site, overriding, overridden) {
                continue;
            }
            visited.insert(overridden);
            let in_superclass = |o: Option<SymbolId>| o.is_some_and(|o| inherited.contains(&o));
            let handled = in_superclass(owners.0)
                && in_superclass(owners.1)
                && super_site.is_some_and(|s| matching(l, s, overriding, overridden));
            if !handled {
                pairs.push((overriding, overridden));
            }
        }
    }
    pairs
}

/// Whether `a` overrides `b` as members of `site`: their parameters match unless one is a value
/// or neither takes parameters.
fn matching(l: &mut Lower, site: tasty::TypeId, a: SymbolId, b: SymbolId) -> bool {
    let methods = l.u.symbol(a).kind == Kind::Def && l.u.symbol(b).kind == Kind::Def;
    if !methods {
        return true;
    }
    let clauses = |l: &mut Lower, s| {
        let info = l.u.info(s);
        l.u.param_clauses(info).iter().map(|(_, ps)| ps.len()).collect::<Vec<_>>()
    };
    let (ca, cb) = (clauses(l, a), clauses(l, b));
    let nullary = |c: &[usize]| c.is_empty() || c == [0];
    (nullary(&ca) && nullary(&cb)) || l.u.matches(site, a, b)
}

/// Whether `s` is a non-private method or value a class can override.
fn is_member(l: &Lower, s: SymbolId) -> bool {
    matches!(l.u.symbol(s).kind, Kind::Def | Kind::Val) && !has(l.u, s, PRIVATE) && !l.u.is_constructor(s)
}

/// Whether method `m` has no body: one of the unit's in `deferred`, or the library's.
fn is_deferred(l: &Lower, deferred: &FxHashSet<SymbolId>, m: SymbolId) -> bool {
    deferred.contains(&m) || l.u.is_deferred(m) || l.u.symbol(m).tree.is_some() && l.u.body(m).is_none()
}

/// `def m(params): R = this.m(params)` with the type of `other` that `m` overrides.
fn bridge(l: &mut Lower, class: SymbolId, m: SymbolId, other: SymbolId, span: Span) -> Result<TNodeId> {
    let mut info = l.u.info(other);
    if l.u.symbol(other).owner.is_some_and(|o| is_jdk(l, o)) {
        info = with_java_param_names(l, info);
    }
    let m_info = l.u.info(m);
    let m_result = l.u.result_type(m_info);
    let mut flags = l.u.symbol(m).flags;
    for tag in [FIELDaccessor, PARAMsetter, CASEaccessor, LAZY, OBJECT] {
        flags = flags.without(tag);
    }
    let (params, other_params) = (l.u.erased_method(m).0, l.u.erased_method(other).0);
    let value_params = l.u.erased_method_value(m).0;
    let other_result = l.u.erased_method_value(other).1;
    let previous = l.erasure_owner.replace(m);
    let made = l.method_like(class, m, flags, info, span, |l, args, _| {
        let args: Vec<TNodeId> = args
            .iter()
            .zip(params.iter().zip(&other_params).zip(&value_params))
            .map(|(&a, ((p, o), v))| if p == o { a } else { adapt(l, a, v) })
            .collect();
        let call = if l.u.is_module(m) && is_static_module(l.u, class) {
            l.ident(m, span)
        } else {
            let this = l.this(class, span);
            let fun = l.select(this, m, span);
            l.apply(fun, &args, m_result, span)
        };
        adapt(l, call, &other_result)
    });
    l.erasure_owner = previous;
    let (sym, def) = made?;
    for t in super::walk::subtrees(l.tpd, def) {
        let local = l
            .tpd
            .sym(t)
            .filter(|&s| matches!(l.tpd.tree(t), Tree::ValDef { .. }) && l.u.symbol(s).owner == Some(m));
        if let Some(s) = local {
            l.u.set_owner(s, sym);
        }
    }
    l.bridges.insert(sym);
    Ok(def)
}
