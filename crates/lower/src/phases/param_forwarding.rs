// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ParamForwarding.scala
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//! dotty's `ParamForwarding` with `PostTyper.forwardParamAccessors`: a private class parameter
//! the superclass call passes to the like-named parameter of the superclass becomes
//! `private def x$accessor = super.x`, `x` the first accessor up the chain the class can reach.

use super::walk::children;
use crate::Lower;
use crate::definitions::symbol;
use crate::symbols::has;
use rustc_hash::FxHashSet;
use tasty::tags::{LOCAL, MUTABLE, PARAMalias, PARAMsetter, PRIVATE, PRIVATEqualified, STABLE};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Template, Tree};

/// A parameter accessor `sym` defined by `stat` of class definition `td`, and the accessor of a
/// superclass it forwards to.
struct Forward {
    td: TNodeId,
    stat: TNodeId,
    sym: SymbolId,
    alias: SymbolId,
}

pub fn transform(l: &mut Lower, root: TNodeId, classes: &[TNodeId]) {
    let candidates: Vec<(TNodeId, TNodeId, SymbolId)> = classes
        .iter()
        .flat_map(|&td| super_param_aliases(l, td).into_iter().map(move |(s, y)| (td, s, y)))
        .collect();
    let aliases: FxHashSet<SymbolId> = candidates.iter().map(|&(_, _, s)| s).collect();
    let forwards: Vec<Forward> = candidates
        .into_iter()
        .filter_map(|(td, stat, sym)| {
            let class = l.tpd.sym(td)?;
            let alias = inherited_accessor(l, sym, class, &aliases)?;
            Some(Forward { td, stat, sym, alias })
        })
        .collect();
    for f in forwards {
        forward(l, root, &f);
    }
}

/// The definitions and symbols of the private immutable parameter accessors of class definition
/// `td` its superclass call passes to the superclass's parameter of the same name (dotty's
/// `SuperParamAlias`).
fn super_param_aliases(l: &mut Lower, td: TNodeId) -> Vec<(TNodeId, SymbolId)> {
    let Some(Template { parents, body, .. }) = l.tpd.class_template(td) else { return Vec::new() };
    let Some(&call) = l.tpd.items(parents).first() else { return Vec::new() };
    let Tree::Apply { fun, args } = *l.tpd.tree(call) else { return Vec::new() };
    let args = l.tpd.items(args).to_vec();
    let (mut f, mut clause) = (fun, 0);
    let ctor = loop {
        match *l.tpd.tree(f) {
            Tree::Apply { fun, .. } => (f, clause) = (fun, clause + 1),
            Tree::TypeApply { fun, .. } => f = fun,
            _ => break l.tpd.sym(f),
        }
    };
    let Some(ctor) = ctor.filter(|&c| !args.is_empty() && is_primary_constructor(l, c)) else {
        return Vec::new();
    };
    let names = clause_names(l, ctor, clause);
    let mut out = Vec::new();
    for &stat in l.tpd.items(body) {
        let (Tree::ValDef { .. }, Some(sym)) = (l.tpd.tree(stat), l.tpd.sym(stat)) else { continue };
        if !has(l.u, sym, PRIVATE) || !has(l.u, sym, PARAMsetter) || has(l.u, sym, MUTABLE) {
            continue;
        }
        let Some(i) = args.iter().position(|&a| l.tpd.sym(a) == Some(sym)) else { continue };
        if names.get(i).is_some_and(|n| *n == l.u.symbol(sym).name) {
            out.push((stat, sym));
        }
    }
    out
}

fn is_primary_constructor(l: &mut Lower, ctor: SymbolId) -> bool {
    let Some(class) = l.u.symbol(ctor).owner else { return false };
    l.u.decl(class, "<init>").unwrap_or_default().first() == Some(&ctor)
}

/// The parameter names of clause `n` of constructor `ctor`.
fn clause_names(l: &mut Lower, ctor: SymbolId, n: usize) -> Vec<String> {
    let mut t = l.u.info(ctor);
    let mut clauses = Vec::new();
    loop {
        match l.u.types().get(t) {
            Type::Poly { result, .. } => t = *result,
            Type::Method { params, result, .. } => {
                clauses.push(params.iter().map(|(name, _)| name.clone()).collect());
                t = *result;
            }
            _ => break,
        }
    }
    clauses.into_iter().nth(n).unwrap_or_default()
}

/// The parameter accessor of the superclass of `sym`'s class that `class` reaches through
/// `super`, following the chain of `aliases` (dotty's `inheritedAccessor`).
fn inherited_accessor(
    l: &mut Lower,
    sym: SymbolId,
    class: SymbolId,
    aliases: &FxHashSet<SymbolId>,
) -> Option<SymbolId> {
    let owner = l.u.symbol(sym).owner?;
    let superclass = superclass(l, owner)?;
    let name = l.u.symbol(sym).name.clone();
    let decls = l.u.decl(superclass, &name).unwrap_or_default();
    let is_accessor = |l: &Lower, c| has(l.u, c, PARAMsetter) || l.u.symbol(c).kind == Kind::Param;
    let candidate = decls.into_iter().find(|&c| is_accessor(l, c) && !has(l.u, c, MUTABLE))?;
    if !has(l.u, candidate, PRIVATE) && is_accessible(l, candidate, class) {
        Some(candidate)
    } else if aliases.contains(&candidate) || has(l.u, candidate, PARAMalias) {
        inherited_accessor(l, candidate, class, aliases)
    } else {
        None
    }
}

fn superclass(l: &mut Lower, c: SymbolId) -> Option<SymbolId> {
    let parent = *l.u.parents(c).first()?;
    l.u.class_symbol(parent).filter(|&s| !l.u.is_trait(s))
}

/// Whether non-private `m` is accessible in `class`: inside its qualifier if it has one.
fn is_accessible(l: &mut Lower, m: SymbolId, class: SymbolId) -> bool {
    if !has(l.u, m, PRIVATEqualified) {
        return true;
    }
    let Some(within) = l.u.access_qualifier(m) else { return true };
    std::iter::successors(Some(class), |&c| l.u.symbol(c).owner).any(|c| c == within)
}

/// Replaces `f.stat` by `private def x$accessor = super.x` and makes the references to `f.sym`
/// in unit `root` but its class's superclass call refer to it.
fn forward(l: &mut Lower, root: TNodeId, f: &Forward) {
    let class = l.u.symbol(f.sym).owner.expect("a parameter accessor's class");
    let span = l.tpd.span(f.stat);
    let name = format!("{}$accessor", l.u.symbol(f.sym).name);
    let mut flags = Flags::default().with(PRIVATE).with(STABLE);
    if has(l.u, f.sym, LOCAL) {
        flags = flags.with(LOCAL);
    }
    let acc = l.u.define(symbol(&name, Kind::Def, class, flags));
    let info = l.u.info(f.sym);
    l.u.set_info(acc, info);
    let parent = l.u.parents(class)[0];
    let sup = l.super_(class, parent, span);
    let sel = l.select(sup, f.alias, span);
    let tpt = l.type_tree(info, span);
    let def = l.build().def_def(acc, &[], tpt, Some(sel), span);
    l.u.unenter(f.sym);
    let Some(Template { parents, body, .. }) = l.tpd.class_template(f.td) else { unreachable!("a class") };
    let calls = l.tpd.items(parents).to_vec();
    let stats: Vec<TNodeId> = l.tpd.items(body).iter().map(|&s| if s == f.stat { def } else { s }).collect();
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(f.td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, .. } = *l.tpd.tree(template) else {
        unreachable!("a template")
    };
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    redirect(l, root, &calls, f.sym, acc);
}

/// Makes the references to `from` under `t` outside `skipped` refer to `to`.
fn redirect(l: &mut Lower, t: TNodeId, skipped: &[TNodeId], from: SymbolId, to: SymbolId) {
    let mut todo = vec![t];
    while let Some(t) = todo.pop() {
        todo.extend(children(l.tpd, t).into_iter().filter(|c| !skipped.contains(c)));
        if l.tpd.sym(t) != Some(from) {
            continue;
        }
        let name = l.term_name(&l.u.symbol(to).name.clone());
        let tree = match *l.tpd.tree(t) {
            Tree::Ident { .. } => Tree::Ident { name },
            Tree::Select { qualifier, .. } => Tree::Select { qualifier, name },
            _ => continue,
        };
        let tpe = match l.tpd.get(t).tpe.map(|tpe| l.u.types().get(tpe).clone()) {
            Some(Type::Sym(prefix, s)) if s == from => Some(l.u.intern_type(Type::Sym(prefix, to))),
            _ => l.tpd.get(t).tpe,
        };
        let node = l.tpd.get_mut(t);
        (node.tree, node.sym, node.tpe) = (tree, Some(to), tpe);
    }
}
