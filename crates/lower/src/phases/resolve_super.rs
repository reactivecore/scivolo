// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ResolveSuper.scala
//! dotty's `ResolveSuper`: a class implements the super accessors of the traits it mixes in,
//! each calling the member it accesses in the first base class after the trait that has one.

use super::mixin::{has_body, mixins};
use crate::definitions::symbol;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{ARTIFACT, PRIVATE};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Adds to class definition `td` the implementations of the super accessors of the traits its
/// class mixes in, first in its body.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    if l.u.is_trait(class) {
        return Ok(());
    }
    let span = l.tpd.span(td);
    let at = if span.exists() { Span::at(span.point()) } else { span };
    let mut defs = Vec::new();
    for mixin in mixins(l, class) {
        let library = l.u.symbol(mixin).tree.is_some();
        for acc in l.u.decls(mixin).unwrap_or_default() {
            if l.super_accessors.contains_key(&acc) {
                defs.push(implementation(l, class, acc, at)?);
            } else if library && has(l.u, acc, ARTIFACT) && decompose(l, acc, &l.u.symbol(acc).name).is_some()
            {
                defs.push(library_implementation(l, class, acc, at)?);
            }
        }
    }
    if defs.is_empty() {
        return Ok(());
    }
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    defs.extend_from_slice(l.tpd.items(body));
    let body = l.tpd.list(&defs);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// `def T$$super$m(params) = C.super[S].m(params)` in `class` for super accessor `acc`, at `span`.
fn implementation(l: &mut Lower, class: SymbolId, acc: SymbolId, span: Span) -> Result<TNodeId> {
    let (member, mixed) = l.super_accessors[&acc];
    let target = match mixed {
        true => member,
        false => {
            let m = l.u.symbol(member);
            let (name, target) = (m.name.clone(), m.target.clone().unwrap_or_else(|| m.name.clone()));
            rebind_super(l, class, acc, Wanted { name: &name, mix: "", target: &target }, span)?
        }
    };
    let owner = l.u.symbol(target).owner.expect("a member's owner");
    let flags = l.u.symbol(acc).flags;
    let this = l.u.this_type(class);
    let info = l.u.member_info(this, acc);
    let (sym, def) = l.method_like(class, acc, flags, info, span, |l, args, result| {
        let sup = l.super_mix(class, owner, span);
        let fun = l.select(sup, target, span);
        l.apply(fun, args, result, span)
    })?;
    own_type_params(l, sym, acc);
    Ok(def)
}

/// New type parameters of `sym` for those of its type, in which references to those of `acc`
/// become references to them.
fn own_type_params(l: &mut Lower, sym: SymbolId, acc: SymbolId) {
    let info = l.u.info(sym);
    let (fresh, _) = super::super_accessors::type_params(l, sym, info);
    let old = l.u.type_params(acc);
    if old.len() != fresh.len() {
        return;
    }
    let refs: Vec<TypeId> = fresh.iter().map(|&t| l.u.intern_type(Type::Sym(None, t))).collect();
    let info = l.u.map_type(info, &mut |u, t| match *u.types().get(t) {
        Type::Sym(_, s) => old.iter().position(|&p| p == s).map(|i| refs[i]),
        _ => None,
    });
    l.u.set_info(sym, info);
}

/// The member name and mix of super accessor name `name` of `acc`: `m` and an empty mix for
/// `super$m`, `m` and `Q` for `m$super$Q`, each also with the prefix `T$$` of the accessor's
/// trait `T` (dotty's `decomposeSuperName`).
fn decompose<'a>(l: &Lower, acc: SymbolId, name: &'a str) -> Option<(&'a str, &'a str)> {
    let owner = l.u.symbol(acc).owner?;
    let prefix = format!("{}$$", l.u.expanded_prefix(owner));
    let unexpanded = name.strip_prefix(prefix.as_str()).unwrap_or(name);
    match unexpanded.strip_prefix("super$") {
        Some(member) => Some((member, "")),
        None => unexpanded.rsplit_once("$super$"),
    }
}

/// The implementation in `class` of super accessor `acc` of a library trait, at `span`.
fn library_implementation(l: &mut Lower, class: SymbolId, acc: SymbolId, span: Span) -> Result<TNodeId> {
    let (expanded, renamed) = (l.u.symbol(acc).name.clone(), l.u.symbol(acc).target.clone());
    let (name, mix) = decompose(l, acc, &expanded).expect("a super accessor name");
    let target = match &renamed {
        Some(t) => decompose(l, acc, t).map_or(t.as_str(), |(m, _)| m),
        None => name,
    };
    let target = rebind_super(l, class, acc, Wanted { name, mix, target }, span)?;
    let owner = l.u.symbol(target).owner.expect("a member's owner");
    let this = l.u.this_type(class);
    let info = l.u.member_info(this, acc);
    let sym = l.u.define(symbol(&expanded, Kind::Def, class, Flags::default().with(ARTIFACT)));
    if let Some(t) = &renamed {
        l.u.set_target(sym, t);
    }
    l.u.set_info(sym, info);
    own_type_params(l, sym, acc);
    Ok(l.forwarding_def(sym, span, |l, args, result| {
        let sup = l.super_mix(class, owner, span);
        let fun = l.select(sup, target, span);
        l.apply(fun, args, result, span)
    }))
}

/// The member a super accessor calls: its name, the trait `Q` of `super[Q]` or an empty mix, and
/// its run-time name.
struct Wanted<'a> {
    name: &'a str,
    mix: &'a str,
    target: &'a str,
}

/// The member `w` super accessor `acc` calls in `base`: the first concrete one in the base
/// classes of `base` after the accessor's trait, declared in a class named `w.mix` unless that is
/// empty, whose type seen from `base` matches the accessor's, bridges left out (dotty's
/// `rebindSuper`).
fn rebind_super(l: &mut Lower, base: SymbolId, acc: SymbolId, w: Wanted, span: Span) -> Result<SymbolId> {
    let owner = l.u.symbol(acc).owner;
    let site = l.u.this_type(base);
    let bases = l.u.linearization(base);
    for &b in bases.iter().skip_while(|&&b| Some(b) != owner).skip(1) {
        if !w.mix.is_empty() && l.u.symbol(b).name != w.mix {
            continue;
        }
        for other in l.u.decl(b, w.name).unwrap_or_default() {
            let s = l.u.symbol(other);
            let named = s.target.as_deref().unwrap_or(&s.name) == w.target;
            let candidate =
                named && !has(l.u, other, PRIVATE) && !l.bridges.contains(&other) && has_body(l, other);
            if candidate && l.u.info_matches(site, other, acc) {
                return Ok(other);
            }
        }
    }
    unsupported(span, "a super accessor without a concrete member")
}
