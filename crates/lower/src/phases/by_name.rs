//! Arguments to by-name parameters (dotty's `ElimByName`, its calls): `f(e)` for `f(x: => T)`
//! becomes `f({ def $anonfun(): T = e; closure($anonfun) })`, the closure owned by the
//! definition the call is in.

use super::adapt::from_start;
use super::walk::children;
use crate::Lower;
use crate::definitions::symbol;
use tasty::tags::{ARTIFACT, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// Wraps the arguments to by-name parameters in class definition `td` in closures.
pub fn transform(l: &mut Lower, td: TNodeId) {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    walk(l, td, class, td);
}

/// Wraps the by-name arguments under `t`, `owner` the definition `t` is in.
fn walk(l: &mut Lower, t: TNodeId, owner: SymbolId, td: TNodeId) {
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
    let inner = match (l.tpd.tree(t), l.tpd.sym(t)) {
        (Tree::DefDef { .. } | Tree::ValDef { .. }, Some(s)) => s,
        _ => owner,
    };
    for c in children(l.tpd, t) {
        walk(l, c, inner, td);
    }
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

/// `{ def $anonfun(): T = arg; closure($anonfun) }` of type `() => T`, `T` the widened `formal`.
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
    let closure = l.tpd.node(Tree::Closure { env, meth, tpt: None }, span.end_pos(), Some(function), None);
    l.block(&[def], closure, span)
}
