//! dotty's `SpecializeFunctions`: a class's `apply` of a `FunctionN` with primitive types moves
//! its body to the specialized `apply$mc<R><Ps>$sp` it calls, and `f(x)` for `f: Int => Int`
//! calls `f.apply$mcII$sp(x)`, which needs no boxing.

use super::adapt::from_start;
use super::adapted::specializable;
use super::mixin::function_arity;
use super::walk::{subtrees, walk_class_up};
use crate::definitions::{specialized_apply, symbol};
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use std::ops::ControlFlow;
use syntax::ast::Span;
use tasty::tags::{INLINE, PRIVATE, SYNTHETIC};
use tasty::{Erased, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree, Walk};

/// Specializes the `apply` methods of class definition `td` and the calls of functions' `apply`
/// in it, each method after the calls in it.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let mut result = Ok(());
    let mut added = Vec::new();
    let mut enter = |l: &mut Lower, t, _| {
        if let Tree::Apply { fun, args } = *l.tpd.tree(t)
            && let Err(e) = specialize(l, t, fun, l.tpd.items(args).len())
        {
            result = Err(e);
            return Walk::Stop;
        }
        Walk::Children
    };
    let mut failed = Ok(());
    let mut leave = |l: &mut Lower, t, owner| {
        if owner != class || !matches!(l.tpd.tree(t), Tree::DefDef { .. }) {
            return ControlFlow::Continue(());
        }
        match specialize_def(l, class, t) {
            Ok(Some(def)) => added.push((t, def)),
            Ok(None) => {}
            Err(e) => {
                failed = Err(e);
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    };
    walk_class_up(l, td, &mut enter, &mut leave);
    result?;
    failed?;
    if added.is_empty() {
        return Ok(());
    }
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let mut stats = Vec::new();
    for &stat in l.tpd.items(body) {
        stats.push(stat);
        stats.extend(added.iter().filter(|&&(t, _)| t == stat).map(|&(_, def)| def));
    }
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// Makes call `app` of `fun` with `arity` arguments call the specialized `apply` if `fun` is the
/// `apply` of a function class of specializable primitive types.
fn specialize(l: &mut Lower, app: TNodeId, fun: TNodeId, arity: usize) -> Result<()> {
    let Some(sym) = l.tpd.sym(fun) else { return Ok(()) };
    let owner = l.u.symbol(sym).owner;
    let function = owner.is_some_and(|o| l.u.is_class(o) && is_function(l, o, arity));
    if arity > 2 || l.u.symbol(sym).name != "apply" || !function {
        return Ok(());
    }
    let method = l.u.widen(l.tpd.tpe(fun));
    let Type::Method { params, .. } = l.u.types().get(method).clone() else { return Ok(()) };
    let params: Vec<Erased> = params.iter().map(|&(_, t)| l.u.erase(t)).collect();
    let result = l.u.widen(l.tpd.tpe(app));
    let result = l.u.erase(result);
    let primitives = params.iter().all(|p| matches!(p, Erased::Prim(_)));
    if !primitives || !specializable(&params, &result) {
        return Ok(());
    }
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else {
        return unsupported(l.tpd.span(app), "specialized calls of apply without a qualifier");
    };
    let Ok(specialized) = specialized_apply(l.u, &params, &result) else {
        return unsupported(l.tpd.span(app), "specialized functions");
    };
    let prefix = l.tpd.tpe(qualifier);
    let name = l.u.symbol(specialized).name.clone();
    let receiver = l.u.widen(prefix);
    let own = l.u.class_symbol(receiver).and_then(|c| {
        let bases = l.u.linearization(c);
        bases.into_iter().find_map(|b| l.u.decl(b, &name).ok()?.first().copied())
    });
    let specialized = own.unwrap_or(specialized);
    let tpe = l.u.intern_type(Type::Sym(Some(prefix), specialized));
    let node = l.tpd.get_mut(fun);
    (node.sym, node.tpe) = (Some(specialized), Some(tpe));
    let start = l.tpd.span(qualifier);
    let node = l.tpd.get_mut(app);
    node.span = match start.exists() && node.span.exists() {
        true => Span::new(start.start(), node.span.end(), start.start()),
        false => from_start(node.span),
    };
    Ok(())
}

/// Whether class `c` derives from `Function{arity}`.
fn is_function(l: &mut Lower, c: SymbolId, arity: usize) -> bool {
    l.u.linearization(c).into_iter().any(|b| function_arity(l, b) == Some(arity))
}

/// The specialized `apply` method of `FunctionN` that method definition `dd` of `class`
/// overrides, if `dd` is an `apply` of specializable primitive types.
fn specialized_of(l: &mut Lower, class: SymbolId, dd: TNodeId) -> Option<SymbolId> {
    let Tree::DefDef { rhs: Some(_), .. } = *l.tpd.tree(dd) else { return None };
    specialized_for(l, class, l.tpd.sym(dd)?)
}

/// Whether method `m` with a body is an `apply` this phase gives a specialized copy.
pub fn is_specialized(l: &mut Lower, m: SymbolId) -> bool {
    let class = l.u.symbol(m).owner.filter(|&c| l.u.is_class(c));
    class.is_some_and(|c| specialized_for(l, c, m).is_some())
}

/// The specialized `apply` method of `FunctionN` that method `sym` of `class` overrides.
fn specialized_for(l: &mut Lower, class: SymbolId, sym: SymbolId) -> Option<SymbolId> {
    let info = l.u.info(sym);
    let clauses = l.u.param_clauses(info);
    let [(_, param_types)] = clauses.as_slice() else { return None };
    if l.u.symbol(sym).name != "apply" || has(l.u, sym, INLINE) || !is_function(l, class, param_types.len()) {
        return None;
    }
    let params: Option<Vec<Erased>> = param_types.iter().map(|&t| primitive(l, t)).collect();
    let result_type = l.u.result_type(info);
    let (params, result) = (params?, primitive(l, result_type)?);
    specializable(&params, &result).then(|| specialized_apply(l.u, &params, &result).ok()).flatten()
}

/// For method definition `dd` of `class`, an `apply` of specializable primitive types: makes it
/// call the specialized method; the definition of that method, with the body of `dd`.
fn specialize_def(l: &mut Lower, class: SymbolId, dd: TNodeId) -> Result<Option<TNodeId>> {
    let Some(like) = specialized_of(l, class, dd) else { return Ok(None) };
    let Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) } = *l.tpd.tree(dd) else { return Ok(None) };
    if subtrees(l.tpd, rhs).into_iter().any(|t| matches!(l.tpd.tree(t), Tree::Closure { .. })) {
        return unsupported(l.tpd.span(dd), "lambdas in specialized apply methods");
    }
    let sym = l.tpd.sym(dd).expect("a method's symbol");
    let info = l.u.info(sym);
    let result_type = l.u.result_type(info);
    let spec_name = l.u.symbol(like).name.clone();
    let flags = l.u.symbol(sym).flags.with(SYNTHETIC).without(PRIVATE);
    let spec = l.u.define(symbol(&spec_name, Kind::Def, class, flags));
    l.u.set_info(spec, info);
    let old_params: Vec<TNodeId> =
        l.tpd.clause_items(paramss).iter().flat_map(|&c| l.tpd.items(c).to_vec()).collect();
    let mut new_params = Vec::new();
    let mut args = Vec::new();
    for &p in &old_params {
        let Some(old) = l.tpd.sym(p) else { continue };
        let fresh = l.u.define(symbol(&l.u.symbol(old).name.clone(), Kind::Param, sym, Default::default()));
        let p_info = l.u.info(old);
        l.u.set_info(fresh, p_info);
        l.u.set_owner(old, spec);
        new_params.push(l.param_def(fresh, l.tpd.span(p)));
        args.push(l.ident(fresh, Span::NONE));
        l.tpd.get_mut(p).span = Span::NONE;
    }
    l.specialized.insert(spec);
    let rhs_span = l.tpd.span(rhs).to_synthetic();
    let def = l.build().def_def(spec, &[&old_params], tpt, Some(rhs), rhs_span);
    let this = l.this(class, Span::NONE);
    let fun = l.select(this, spec, Span::NONE);
    let call = l.apply(fun, &args, result_type, Span::NONE);
    let clause = l.tpd.list(&new_params);
    let paramss = l.tpd.clauses(&[clause]);
    l.tpd.get_mut(dd).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(call) };
    l.recopy(dd);
    Ok(Some(def))
}

/// The symbol type `t` refers to, an alias not dealiased (dotty's `typeSymbol`).
fn type_symbol(l: &mut Lower, t: TypeId) -> Option<SymbolId> {
    match l.u.types().get(t).clone() {
        Type::Sym(_, s) => Some(s),
        Type::TypeRef(prefix, name) => l.u.resolve(prefix, &name, true),
        Type::Annotated(inner, _) => type_symbol(l, inner),
        _ => None,
    }
}

/// The erasure of `t` if its type symbol, not dealiased, is a primitive class or `Unit`.
fn primitive(l: &mut Lower, t: TypeId) -> Option<Erased> {
    let class = type_symbol(l, t)?;
    if l.u.symbol(class).kind != Kind::Class {
        return None;
    }
    let letter = match l.u.full_name(class).as_str() {
        "scala.Unit" => return Some(Erased::Unit),
        "scala.Boolean" => 'z',
        "scala.Byte" => 'b',
        "scala.Short" => 's',
        "scala.Char" => 'c',
        "scala.Int" => 'i',
        "scala.Long" => 'j',
        "scala.Float" => 'f',
        "scala.Double" => 'd',
        _ => return None,
    };
    Some(Erased::Prim(letter))
}
