// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//! The parameter accessors a class passes on to the like-named parameters of its superclass
//! (dotty's `PostTyper.forwardParamAccessors`).

use crate::tpd::{TNodeId, Template, Tpd, Tree};
use names::nme;
use tasty::tags::{MUTABLE, PARAMsetter, PRIVATE};
use tasty::{NameId, SymbolId, Type, Universe};

/// Lists the definitions and symbols of the private immutable parameter accessors of class
/// template `template` that its superclass call passes to the superclass's parameter of the
/// same name, as `x` in `class B(x: Int) extends A(x)` of `class A(val x: Int)` (dotty's
/// `forwardParamAccessors`, which flags them `SuperParamAlias`). Only the arguments of the
/// call's last clause count, a by-name accessor never.
pub fn super_param_aliases(u: &mut Universe, tpd: &Tpd, template: &Template) -> Vec<(TNodeId, SymbolId)> {
    let &Template { parents, body, .. } = template;
    let Some(&call) = tpd.items(parents).first() else { return Vec::new() };
    let Tree::Apply { fun, args } = *tpd.tree(call) else { return Vec::new() };
    let args = tpd.items(args).to_vec();
    let (mut f, mut clause) = (fun, 0);
    let ctor = loop {
        match *tpd.tree(f) {
            Tree::Apply { fun, .. } => (f, clause) = (fun, clause + 1),
            Tree::TypeApply { fun, .. } => f = fun,
            _ => break tpd.sym(f),
        }
    };
    let Some(ctor) = ctor.filter(|&c| !args.is_empty() && is_primary_constructor(u, c)) else {
        return Vec::new();
    };

    let names = clause_names(u, ctor, clause);
    let mut out = Vec::new();
    for &stat in tpd.items(body) {
        let (Tree::ValDef { .. }, Some(sym)) = (tpd.tree(stat), tpd.sym(stat)) else { continue };
        let flags = u.symbol(sym).flags;
        if !flags.has(PRIVATE) || !flags.has(PARAMsetter) || flags.has(MUTABLE) {
            continue;
        }
        let info = u.info(sym);
        if u.by_name(info).is_some() {
            continue;
        }
        let Some(i) = args.iter().position(|&a| tpd.sym(a) == Some(sym)) else { continue };
        if names.get(i).is_some_and(|&n| u.name(n) == u.symbol(sym).name) {
            out.push((stat, sym));
        }
    }
    out
}

fn is_primary_constructor(u: &mut Universe, ctor: SymbolId) -> bool {
    let Some(class) = u.symbol(ctor).owner else { return false };
    u.decls_named(class, nme::CONSTRUCTOR).first() == Some(&ctor)
}

/// The parameter names of clause `n` of constructor `ctor`.
fn clause_names(u: &mut Universe, ctor: SymbolId, n: usize) -> Vec<NameId> {
    let mut t = u.info(ctor);
    let mut clauses = Vec::new();
    loop {
        match *u.ty(t) {
            Type::Poly { result, .. } => t = result,
            Type::Method { params, result, .. } => {
                clauses.push(u.params_of(params).iter().map(|&(name, _)| name).collect());
                t = result;
            }
            _ => break,
        }
    }
    clauses.into_iter().nth(n).unwrap_or_default()
}
