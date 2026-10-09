//! dotty's `ElimByName`: `f(e)` for `f(x: => T)` becomes
//! `f({ def $anonfun(): T = e; closure($anonfun) })`, the closure owned by the definition the
//! call is in, unless `e` is itself a by-name parameter, passed on as is; any other reference to
//! a by-name parameter `x` becomes `x.apply()`.

use super::adapt::from_start;
use super::getters::is_param_accessor;
use super::walk::children;
use crate::Lower;
use crate::definitions::symbol;
use rustc_hash::FxHashSet;
use syntax::ast::Span;
use tasty::tags::{ARTIFACT, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Rewrites the by-name arguments and references in class definition `td`.
pub fn transform(l: &mut Lower, td: TNodeId) {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    walk(l, td, class, td, &mut FxHashSet::default());
}

/// Rewrites the by-name arguments and references under `t`, `owner` the definition `t` is in,
/// except the references in `passed`, which are arguments to by-name parameters.
fn walk(l: &mut Lower, t: TNodeId, owner: SymbolId, td: TNodeId, passed: &mut FxHashSet<TNodeId>) {
    if matches!(l.tpd.tree(t), Tree::TypeDef { .. }) && t != td {
        return;
    }
    if let Tree::Apply { fun, args } = *l.tpd.tree(t) {
        let old = l.tpd.items(args).to_vec();
        let (formals, instantiated) = formals(l, fun);
        let new: Vec<TNodeId> = old
            .iter()
            .enumerate()
            .map(|(i, &a)| match formals.get(i).map(|&f| l.u.types().get(f).clone()) {
                Some(Type::ByName(_)) if by_name_ref(l, l.tpd.strip_typed(a)) => {
                    let a = l.tpd.strip_typed(a);
                    passed.insert(a);
                    a
                }
                Some(Type::ByName(result)) => {
                    let result = if instantiated { result } else { l.tpd.tpe(a) };
                    closure(l, a, owner, result)
                }
                _ => a,
            })
            .collect();
        if new != old {
            let args = l.tpd.list(&new);
            l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
        }
    }
    if by_name_ref(l, t) && !passed.contains(&t) {
        apply_reference(l, t);
        return;
    }
    let inner = match (l.tpd.tree(t), l.tpd.sym(t)) {
        (Tree::DefDef { .. } | Tree::ValDef { .. }, Some(s)) => s,
        _ => owner,
    };
    for c in children(l.tpd, t) {
        walk(l, c, inner, td, passed);
    }
}

/// Whether `s` is a by-name parameter, of a method or a class (dotty's `exprBecomesFunction`).
pub fn is_by_name_param(l: &mut Lower, s: SymbolId) -> bool {
    let param =
        l.u.symbol(s).kind == Kind::Param || l.u.symbol(s).kind == Kind::Val && is_param_accessor(l, s);
    let info = l.u.info(s);
    param && matches!(l.u.types().get(info), Type::ByName(_))
}

/// Whether `t` refers to a by-name parameter.
fn by_name_ref(l: &mut Lower, t: TNodeId) -> bool {
    let (Tree::Ident { .. } | Tree::Select { .. }, Some(s)) = (l.tpd.tree(t), l.tpd.sym(t)) else {
        return false;
    };
    is_by_name_param(l, s)
}

/// Makes reference `t` to by-name parameter `x: => T` the call `x.apply()` of type `T`.
fn apply_reference(l: &mut Lower, t: TNodeId) {
    let x = l.tpd.sym(t).expect("a by-name parameter");
    let span = l.tpd.span(t);
    let info = l.u.info(x);
    let Type::ByName(result) = *l.u.types().get(info) else { unreachable!("a by-name type") };
    let n = l.tpd.get(t);
    let (tree, tpe) = (n.tree.clone(), n.tpe);
    let qual = l.tpd.node(tree, span, tpe, Some(x));
    let function = l.u.class_ref("scala.Function0");
    let class = l.u.class_symbol(function).expect("`Function0`");
    let apply = l.u.decl(class, "apply").unwrap_or_default();
    let apply = apply.into_iter().next().expect("`Function0.apply`");
    let fun = l.select(qual, apply, Span::NONE);
    let call = l.apply(fun, &[], result, Span::NONE);
    let call = l.with_span(call, span);
    l.copy_node(t, call);
}

/// The parameter types of the clause the arguments of a call of `fun` are for, and whether they
/// are as the call instantiates them rather than as the method declares them.
fn formals(l: &mut Lower, mut fun: TNodeId) -> (Vec<tasty::TypeId>, bool) {
    let direct = l.u.widen(l.tpd.tpe(fun));
    if let Type::Method { params, .. } = l.u.types().get(direct) {
        return (params.iter().map(|&(_, p)| p).collect(), true);
    }
    let mut clause = 0;
    let method = loop {
        match *l.tpd.tree(fun) {
            Tree::Apply { fun: f, .. } => (fun, clause) = (f, clause + 1),
            Tree::TypeApply { fun: f, .. } => fun = f,
            _ => break l.tpd.tpe(fun),
        }
    };
    let mut t = l.u.widen(method);
    loop {
        match l.u.types().get(t).clone() {
            Type::Poly { result, .. } => t = result,
            Type::Method { result, .. } if clause > 0 => (t, clause) = (result, clause - 1),
            Type::Method { params, .. } => return (params.into_iter().map(|(_, p)| p).collect(), false),
            _ => return (Vec::new(), false),
        }
    }
}

/// `{ def $anonfun(): T = arg; closure($anonfun) }` of type `() => T`, `T` the widened `formal`,
/// its target type explicit as dotty's `ByNameFunction`.
fn closure(l: &mut Lower, arg: TNodeId, owner: SymbolId, formal: tasty::TypeId) -> TNodeId {
    let span = l.tpd.span(arg);
    let result = l.u.widen(formal);
    let flags = Flags::default().with(SYNTHETIC).with(ARTIFACT);
    let anon = l.u.define(symbol("$anonfun", Kind::Def, owner, flags));
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method {
        binder,
        params: Vec::new(),
        result,
        implicit: false,
        contextual: false,
    });
    l.u.set_info(anon, info);
    let tpt = l.type_tree(result, span);
    let def = l.method_def(anon, &[], tpt, arg, from_start(span));
    let function = l.u.class_ref("scala.Function0");
    let function = l.u.intern_type(Type::Applied(function, vec![result]));
    let meth = l.ident(anon, span);
    let env = l.tpd.list(&[]);
    let target = Some(l.type_tree(function, Span::NONE));
    let closure = l.tpd.node(Tree::Closure { env, meth, tpt: target }, span.end_pos(), Some(function), None);
    l.block(&[def], closure, span)
}
