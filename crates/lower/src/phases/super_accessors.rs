//! dotty's `SuperAccessors`: `C.super[M].m` in a trait `C` without `M`, or in a class nested in
//! `C`, becomes `C.this.acc` of a super accessor `C` declares, as does `C.this.m` of a protected
//! `m` whose protected accessor trait `C` would host. A trait's accessors have no body and the
//! classes mixing the trait in implement them (`resolve_super.rs`); a class's call `m`.

use super::mixin::{has_body, java_named};
use super::protected_accessors::super_accessor_needed;
use super::walk;
use crate::definitions::symbol;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::{ABSTRACT, ARTIFACT, INLINE, OVERRIDE};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// A super selection: its tree, the class whose `super` it selects on, the mixin's name if
/// given, the member it selects.
struct SuperSelect {
    sel: TNodeId,
    clazz: SymbolId,
    mix: String,
    member: SymbolId,
}

pub fn transform(l: &mut Lower, classes: &[TNodeId]) -> Result<()> {
    let mut defs: FxHashMap<SymbolId, Vec<TNodeId>> = FxHashMap::default();
    let points: FxHashMap<SymbolId, Span> = classes
        .iter()
        .map(|&td| (l.tpd.sym(td).expect("a class definition's symbol"), Span::at(l.tpd.span(td).point())))
        .collect();
    let selects: Vec<(SymbolId, Vec<SuperSelect>)> = classes
        .iter()
        .map(|&td| Ok((l.tpd.sym(td).expect("a class definition's symbol"), super_selects(l, td)?)))
        .collect::<Result<_>>()?;
    for (current, ss) in &selects {
        for s in ss {
            let span = l.tpd.span(s.sel);
            if matches!(l.u.symbol(s.member).kind, Kind::Val | Kind::Param) {
                return unsupported(span, "super calls of values");
            }
            let abstract_override = s.mix.is_empty() && is_abstract_override(l, s.clazz, s.member);
            if !has_body(l, s.member) && !abstract_override {
                return unsupported(span, "super calls of abstract members");
            }
            if s.clazz == *current && !(s.mix.is_empty() && l.u.is_trait(s.clazz)) {
                continue;
            }
            let point = match l.u.is_trait(s.clazz) {
                true => Span::at(span.point()),
                false => points.get(&s.clazz).copied().unwrap_or(Span::NONE),
            };
            let acc = accessor(l, s, point, defs.entry(s.clazz).or_default())?;
            let this = l.this(s.clazz, Span::NONE);
            let sel = l.select(this, acc, Span::NONE);
            l.with_span(sel, span);
            let (tree, tpe) = (l.tpd.tree(sel).clone(), l.tpd.get(sel).tpe);
            let node = l.tpd.get_mut(s.sel);
            (node.tree, node.tpe, node.span, node.sym) = (tree, tpe, span, Some(acc));
        }
    }
    for &td in classes {
        let class = l.tpd.sym(td).expect("a class definition's symbol");
        if let Some(accessors) = defs.remove(&class) {
            add_after_params(l, td, accessors);
        }
    }
    Ok(())
}

/// The term selections on `super` in class definition `td`, outside its nested classes, and those
/// on `this` of protected members whose accessor a trait would host, in the order dotty's
/// `PostTyper` visits them.
fn super_selects(l: &mut Lower, td: TNodeId) -> Result<Vec<SuperSelect>> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let mut out = Vec::new();
    for t in post_typer_order(l, td) {
        let Tree::Select { qualifier, name } = *l.tpd.tree(t) else { continue };
        let (Some(clazz), Some(member)) = (l.tpd.sym(qualifier), l.tpd.sym(t)) else { continue };
        if name.is_type || has(l.u, member, INLINE) {
            continue;
        }
        let mix = match *l.tpd.tree(qualifier) {
            Tree::Super { mix, .. } => match *l.tpd.tree(mix) {
                Tree::Ident { name: mix } => l.names.text(mix.name).to_owned(),
                _ => continue,
            },
            Tree::This { .. } => match super_accessor_needed(l, member, class, l.tpd.span(t))? {
                true => String::new(),
                false => continue,
            },
            _ => continue,
        };
        out.push(SuperSelect { sel: t, clazz, mix, member });
    }
    Ok(out)
}

/// The trees of class definition `td` but type trees and nested classes, in pre-order, the
/// arguments of an application before its function.
fn post_typer_order(l: &Lower, td: TNodeId) -> Vec<TNodeId> {
    let mut out = Vec::new();
    let mut todo = vec![td];
    while let Some(t) = todo.pop() {
        out.push(t);
        let mut kids = match *l.tpd.tree(t) {
            Tree::SingletonTypeTree { .. } | Tree::AppliedTypeTree { .. } | Tree::TypeTree { .. } => continue,
            Tree::TypeDef { .. } if t != td => continue,
            Tree::Apply { fun, args } => {
                let mut kids = l.tpd.items(args).to_vec();
                kids.push(fun);
                kids
            }
            _ => walk::children(l.tpd, t),
        };
        kids.reverse();
        todo.extend(kids);
    }
    out
}

/// Whether `clazz` declares a member of the name of `sym` that is `abstract override`.
fn is_abstract_override(l: &mut Lower, clazz: SymbolId, sym: SymbolId) -> bool {
    let name = l.u.symbol(sym).name.clone();
    let decls = l.u.decl(clazz, &name).unwrap_or_default();
    decls.into_iter().any(|m| has(l.u, m, ABSTRACT) && has(l.u, m, OVERRIDE))
}

/// Puts `stats` into the body of class definition `td` after its type members and parameter
/// accessors.
fn add_after_params(l: &mut Lower, td: TNodeId, stats: Vec<TNodeId>) {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return };
    let old = l.tpd.items(body).to_vec();
    let params = old.iter().take_while(|&&s| is_param(l, s)).count();
    let mut out = old[..params].to_vec();
    out.extend(stats);
    out.extend_from_slice(&old[params..]);
    let body = l.tpd.list(&out);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
}

/// Whether class statement `s` is a type member or a parameter accessor.
fn is_param(l: &Lower, s: TNodeId) -> bool {
    match l.tpd.tree(s) {
        Tree::TypeDef { rhs, .. } => !matches!(l.tpd.tree(*rhs), Tree::Template { .. }),
        Tree::ValDef { .. } | Tree::DefDef { .. } => {
            l.tpd.sym(s).is_some_and(|p| has(l.u, p, tasty::tags::PARAMsetter))
        }
        _ => false,
    }
}

/// The super accessor of `s.clazz` for `s`: the one already there, else a new one whose
/// definition joins `defs`, at `span`.
fn accessor(l: &mut Lower, s: &SuperSelect, span: Span, defs: &mut Vec<TNodeId>) -> Result<SymbolId> {
    let original = l.u.symbol(s.member).name.clone();
    let unexpanded = match s.mix.as_str() {
        "" => format!("super${original}"),
        mix => format!("{original}$super${mix}"),
    };
    let trait_ = l.u.is_trait(s.clazz);
    let name = match trait_ {
        true => format!("{}$${unexpanded}", super::accessor_names::expanded_prefix(l, s.clazz)),
        false => unexpanded,
    };
    let info = l.u.widen(l.tpd.tpe(s.sel));
    let info = java_named(l, s.member, info);
    let declared = l.u.decl(s.clazz, &name).unwrap_or_default();
    if let Some(acc) = declared.into_iter().find(|&a| l.u.info(a) == info) {
        return Ok(acc);
    }
    if matches!(l.u.types().get(info), Type::Poly { .. }) {
        return unsupported(span, "super calls of polymorphic methods");
    }
    let acc = l.u.define(symbol(&name, Kind::Def, s.clazz, Flags::default().with(ARTIFACT)));
    l.u.set_info(acc, info);
    l.super_accessors.insert(acc, (s.member, !s.mix.is_empty()));
    let def = match trait_ {
        true => {
            l.u.set_deferred(acc, true);
            deferred_def(l, acc, info, span)
        }
        false => {
            let (clazz, member) = (s.clazz, s.member);
            let owner = l.u.symbol(member).owner.expect("a member's owner");
            l.forwarding_def(acc, span, |l, args, result| {
                let sup = l.super_mix(clazz, owner, span);
                let fun = l.select(sup, member, span);
                l.apply(fun, args, result, span)
            })
        }
    };
    defs.push(def);
    Ok(acc)
}

/// The definition without a body of method `acc` of type `info`.
fn deferred_def(l: &mut Lower, acc: SymbolId, info: TypeId, span: Span) -> TNodeId {
    let (params, result) = match l.u.types().get(info).clone() {
        Type::Method { params, result, .. } => (Some(params), result),
        _ => (None, info),
    };
    let mut defs = Vec::new();
    for (pname, ptype) in params.iter().flatten() {
        let p = l.u.define(symbol(pname, Kind::Param, acc, Flags::default()));
        l.u.set_info(p, *ptype);
        defs.push(l.param_def(p, span));
    }
    let tpt = l.type_tree(result, span);
    let paramss: Vec<&[TNodeId]> = if params.is_some() { vec![&defs] } else { vec![] };
    l.build().def_def(acc, &paramss, tpt, None, span)
}
