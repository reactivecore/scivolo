//! dotty's `Mixin` for traits without initialization: a trait keeps its constructor, which
//! becomes `$init$` without a body; a class gets, for each concrete trait method it inherits, a
//! forwarder calling that method on `super`.

use crate::definitions::symbol;
use crate::symbols::{has, is_constructor, is_trait};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashSet;
use tasty::tags::{FIELDaccessor, PRIVATE};
use tasty::{Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// The trait methods of the unit that have a body.
pub fn concrete_trait_methods(l: &Lower, classes: &[TNodeId]) -> FxHashSet<SymbolId> {
    let mut out = FxHashSet::default();
    for &td in classes {
        let Tree::TypeDef { rhs, .. } = *l.tpd.tree(td) else { continue };
        let Tree::Template { body, .. } = *l.tpd.tree(rhs) else { continue };
        for &stat in l.tpd.items(body) {
            if let (Tree::DefDef { rhs: Some(_), .. }, Some(s)) = (l.tpd.tree(stat), l.tpd.sym(stat)) {
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

/// Whether `meth` is a method `class` inherits as it is: the first of its name and erased
/// parameters in the linearization of `class`.
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
            if l.u.symbol(other).kind == Kind::Def && l.u.erased_method(other).0 == params {
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
    let params = match l.u.types().get(info).clone() {
        Type::Method { params, result, .. } if !matches!(l.u.types().get(result), Type::Method { .. }) => {
            params
        }
        Type::Method { .. } | Type::Poly { .. } => {
            return unsupported(
                span,
                "forwarders to methods with type parameters or several parameter lists",
            );
        }
        _ => Vec::new(),
    };
    let name = l.u.symbol(meth).name.clone();
    let flags = l.u.symbol(meth).flags;
    let sym = l.u.define(symbol(&name, Kind::Def, class, flags));
    l.u.set_info(sym, info);
    let mut param_defs = Vec::new();
    let mut args = Vec::new();
    for (pname, ptype) in params {
        let p = l.u.define(symbol(&pname, Kind::Param, sym, Default::default()));
        l.u.set_info(p, ptype);
        let tpt = l.type_tree(ptype, span);
        let pname = l.term_name(&pname);
        param_defs.push(l.tpd.node(Tree::ValDef { name: pname, tpt, rhs: None }, span, None, Some(p)));
        args.push(l.ident(p, span));
    }
    let sup = l.super_mix(class, mixin, span);
    let fun = l.select(sup, meth, span);
    let result = l.u.result_type(info);
    let rhs = l.apply(fun, &args, result, span);
    let clause = l.tpd.list(&param_defs);
    let paramss = if param_defs.is_empty() && !matches!(l.u.types().get(info), Type::Method { .. }) {
        l.tpd.clauses(&[])
    } else {
        l.tpd.clauses(&[clause])
    };
    let tpt = l.type_tree(result, span);
    let name = l.term_name(&name);
    Ok(l.tpd.node(Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) }, span, None, Some(sym)))
}
