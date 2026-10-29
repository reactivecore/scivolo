// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/transform/BetaReduce.scala
//! dotty's `BetaReduce`: `apply` of a function type called on a lambda or on a new anonymous
//! class with only that method becomes the method's body, its parameters bound to the arguments.

use super::first_transform::{is_pure_with_by_name as is_pure, literal_of};
use super::module_purity::is_pure_value;
use super::walk::subtrees;
use crate::definitions::{symbol, type_ref};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use std::ops::ControlFlow;
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::tags::SYNTHETIC;
use tasty::{Erased, Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Template, Tree, Walk, traverse_up};

/// Reduces the applications of lambdas and anonymous function classes in unit `root`, inner ones
/// first.
pub fn transform(l: &mut Lower, root: TNodeId, owner: SymbolId) -> Result<()> {
    let mut result = Ok(());
    traverse_up(
        l,
        root,
        owner,
        &mut |_, _, _| Walk::Children,
        &mut |l, t, owner| match reduce(l, t, owner) {
            Ok(()) => ControlFlow::Continue(()),
            Err(e) => {
                result = Err(e);
                ControlFlow::Break(())
            }
        },
    );
    result
}

/// Makes application `t` in `owner` its reduction, if it has one.
fn reduce(l: &mut Lower, t: TNodeId, owner: SymbolId) -> Result<()> {
    let Tree::Apply { fun, args } = *l.tpd.tree(t) else { return Ok(()) };
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return Ok(()) };
    if l.tpd.sym(fun).is_none_or(|s| l.u.symbol(s).name != "apply") || !is_function_type(l, qualifier) {
        return Ok(());
    }
    let args = l.tpd.items(args).to_vec();
    let mut bindings = Vec::new();
    let Some(reduced) = recur(l, qualifier, &args, owner, &mut bindings)? else { return Ok(()) };
    let span = if expanded_without_bindings(l, qualifier) && bindings.is_empty() {
        l.tpd.span(reduced)
    } else {
        l.tpd.span(t)
    };
    let tpe = l.tpd.tpe(t);
    let reduced = l.ensure_conforms(reduced, tpe);
    let result = if bindings.is_empty() { reduced } else { l.block(&bindings, reduced, Span::NONE) };
    l.copy_node(t, result);
    l.tpd.get_mut(t).span = span;
    Ok(())
}

/// Whether `t`, past ascriptions, was the expansion of an `Inlined` tree without bindings.
fn expanded_without_bindings(l: &Lower, t: TNodeId) -> bool {
    match *l.tpd.tree(t) {
        _ if l.inlined_spans.contains_key(&t) => true,
        Tree::Typed { expr, .. } => expanded_without_bindings(l, expr),
        _ => false,
    }
}

/// Whether the type of `t` is a `FunctionN` or `ContextFunctionN` type.
fn is_function_type(l: &mut Lower, t: TNodeId) -> bool {
    let tpe = l.u.widen(l.tpd.tpe(t));
    l.u.class_symbol(tpe).and_then(|c| super::function_classes::function_class_arity(l, c)).is_some()
}

/// The reduction of the call of function `fun` to `args`, past ascriptions and blocks of pure
/// bindings, those blocks kept around it.
fn recur(
    l: &mut Lower,
    fun: TNodeId,
    args: &[TNodeId],
    owner: SymbolId,
    bindings: &mut Vec<TNodeId>,
) -> Result<Option<TNodeId>> {
    match *l.tpd.tree(fun) {
        Tree::Typed { expr, .. } => recur(l, expr, args, owner, bindings),
        Tree::Block { stats: list, expr } => {
            let stats = l.tpd.items(list).to_vec();
            if let (&[def], Tree::Closure { meth, .. }) = (&stats[..], l.tpd.tree(expr))
                && matches!(l.tpd.tree(def), Tree::DefDef { .. })
                && l.tpd.sym(def).is_some()
                && l.tpd.sym(def) == l.tpd.sym(*meth)
            {
                return application(l, def, args, owner, bindings);
            }
            if let &[def] = &stats[..]
                && let Some(method) = only_method(l, def, expr)
            {
                return application(l, method, args, owner, bindings);
            }
            if !stats.iter().all(|&s| is_pure_binding(l, s)) {
                return Ok(None);
            }
            let Some(reduced) = recur(l, expr, args, owner, bindings)? else { return Ok(None) };
            let tpe = l.tpd.tpe(reduced);
            let node = l.tpd.get_mut(fun);
            (node.tree, node.tpe) = (Tree::Block { stats: list, expr: reduced }, Some(tpe));
            Ok(Some(fun))
        }
        _ => Ok(None),
    }
}

/// Whether statement `s` defines a method or type, or an immutable value of a literal or pure
/// path (dotty's `isPureBinding`).
fn is_pure_binding(l: &Lower, s: TNodeId) -> bool {
    match *l.tpd.tree(s) {
        Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::Import { .. } => true,
        Tree::ValDef { rhs: Some(rhs), .. } => {
            let mutable = l.tpd.sym(s).is_some_and(|v| crate::symbols::has(l.u, v, tasty::tags::MUTABLE));
            !mutable && is_pure_value(l, rhs)
        }
        _ => false,
    }
}

/// The method of anonymous class definition `def` whose body is that method, created by `expr`.
fn only_method(l: &Lower, def: TNodeId, expr: TNodeId) -> Option<TNodeId> {
    let Template { constr, body, .. } = l.tpd.class_template(def)?;
    let Tree::Typed { expr: created, .. } = *l.tpd.tree(expr) else { return None };
    let Tree::Apply { fun, .. } = *l.tpd.tree(created) else { return None };
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return None };
    let no_init = matches!(l.tpd.tree(constr), Tree::DefDef { rhs: None, .. });
    match (l.tpd.tree(qualifier), l.tpd.items(body)) {
        (Tree::New { .. }, &[m]) if no_init && matches!(l.tpd.tree(m), Tree::DefDef { .. }) => Some(m),
        _ => None,
    }
}

/// The body of method definition `def` with its parameters bound to `args` in `owner`, the
/// bindings needed added to `bindings` (dotty's `reduceApplication`); `None` unless `args` fit
/// the one parameter clause.
pub fn application(
    l: &mut Lower,
    def: TNodeId,
    args: &[TNodeId],
    owner: SymbolId,
    bindings: &mut Vec<TNodeId>,
) -> Result<Option<TNodeId>> {
    let Tree::DefDef { paramss, rhs: Some(rhs), .. } = *l.tpd.tree(def) else { return Ok(None) };
    let method = l.tpd.sym(def).expect("a method definition's symbol");
    let &[clause] = l.tpd.clause_items(paramss) else { return Ok(None) };
    let params = l.tpd.items(clause).to_vec();
    if params.len() != args.len() || params.iter().any(|&p| !matches!(l.tpd.tree(p), Tree::ValDef { .. })) {
        return Ok(None);
    }
    let params: Vec<SymbolId> = params.iter().map(|&p| l.tpd.sym(p).expect("a parameter's symbol")).collect();
    for (&arg, &param) in args.iter().zip(&params) {
        let info = l.u.info(param);
        if matches!(l.u.types().get(info), Type::ByName(_)) {
            return unsupported(l.tpd.span(arg), "applications of a lambda with by-name parameters");
        }
    }
    let mut subst = FxHashMap::default();
    for (&arg, &param) in args.iter().zip(&params) {
        let arg = literalized(l, arg);
        if let Tree::Ident { .. } = l.tpd.tree(arg)
            && is_pure(l, arg)
            && let Some(s) = l.tpd.sym(arg).filter(|&s| is_local(l, s))
        {
            subst.insert(param, s);
            continue;
        }
        subst.insert(param, binding(l, arg, param, owner, bindings));
    }
    for t in subtrees(l.tpd, rhs) {
        if let Some(s) = l.tpd.sym(t) {
            if l.u.symbol(s).owner == Some(method) && !subst.contains_key(&s) {
                l.u.set_owner(s, owner);
            }
            if let (Tree::Ident { .. }, Some(&to)) = (l.tpd.tree(t), subst.get(&s)) {
                let tpe = type_ref(l.u, to);
                let node = l.tpd.get_mut(t);
                (node.sym, node.tpe) = (Some(to), Some(tpe));
            }
        }
    }
    fold_constants(l, rhs);
    Ok(Some(rhs))
}

/// `arg` as a literal if it is a pure reference to a constant value (dotty's `constToLiteral`).
fn literalized(l: &mut Lower, arg: TNodeId) -> TNodeId {
    let reference = matches!(l.tpd.tree(arg), Tree::Ident { .. } | Tree::Select { .. });
    let Some(s) = l.tpd.sym(arg).filter(|_| reference && is_pure(l, arg)) else { return arg };
    let span = l.tpd.span(arg);
    literal_of(l, s, span).unwrap_or(arg)
}

/// Whether `s` is a value local to a method.
fn is_local(l: &Lower, s: SymbolId) -> bool {
    l.u.symbol(s).owner.is_some_and(|o| matches!(l.u.symbol(o).kind, Kind::Def | Kind::Val))
}

/// The value `param` of `arg` in `owner`, its definition added to `bindings` unless it is a pure
/// constant or `Unit`.
fn binding(
    l: &mut Lower,
    arg: TNodeId,
    param: SymbolId,
    owner: SymbolId,
    bindings: &mut Vec<TNodeId>,
) -> SymbolId {
    let name = l.u.symbol(param).name.clone();
    let tpe = l.tpd.tpe(arg);
    let constant = matches!(l.u.types().get(tpe), Type::Constant(..) | Type::Str(_));
    let tpe = match l.u.erase(tpe) {
        Erased::Nothing | Erased::Null => l.u.info(param),
        _ if constant => tpe,
        _ => l.u.widen(tpe),
    };
    let sym = l.u.define(symbol(&name, Kind::Val, owner, Flags::default().with(SYNTHETIC)));
    l.u.set_info(sym, tpe);
    let pure = is_pure_value(l, arg);
    if !((constant || l.u.erase(tpe) == Erased::Unit) && pure) {
        let span = l.tpd.span(arg);
        let tpt = l.type_tree(tpe, Span::NONE);
        let term = l.term_name(&name);
        let def =
            l.tpd.node(Tree::ValDef { name: term, tpt, rhs: Some(arg) }, Span::NONE, Some(tpe), Some(sym));
        bindings.push(l.with_span(def, span));
    }
    sym
}

/// Makes the references to constant and `Unit` values under `t` literals, then the operations on
/// literals their results (dotty's `ConstFold` as `StringInterpolatorOpt` applies it), inner
/// trees first.
fn fold_constants(l: &mut Lower, t: TNodeId) {
    for t in subtrees(l.tpd, t).into_iter().rev() {
        let span = l.tpd.span(t);
        let tree = l.tpd.tree(t).clone();
        let folded = match tree {
            Tree::Ident { .. } if is_unit_value(l, t) => Some(literal(l, Constant::Unit, span)),
            Tree::Ident { .. } => l.tpd.sym(t).and_then(|s| literal_of(l, s, span)),
            Tree::Apply { fun, args } => match (l.tpd.tree(fun), l.tpd.items(args)) {
                (&Tree::Select { qualifier, name }, &[arg]) => {
                    let op = l.names.text(name.name).to_owned();
                    match (constant(l, qualifier), constant(l, arg)) {
                        (Some(x), Some(y)) => typer::fold_binop(&op, &x, &y).map(|c| literal(l, c, span)),
                        _ => None,
                    }
                }
                _ => None,
            },
            Tree::Select { qualifier, name } => {
                let op = l.names.text(name.name).to_owned();
                constant(l, qualifier).and_then(|x| typer::fold_unop(&op, &x)).map(|c| literal(l, c, span))
            }
            _ => None,
        };
        if let Some(lit) = folded {
            l.copy_node(t, lit);
        }
    }
}

/// Whether `t` is a pure reference to a value of type `Unit`.
fn is_unit_value(l: &mut Lower, t: TNodeId) -> bool {
    let Some(s) = l.tpd.sym(t).filter(|&s| matches!(l.u.symbol(s).kind, Kind::Val | Kind::Param)) else {
        return false;
    };
    let info = l.u.info(s);
    is_pure_value(l, t) && l.u.erase(info) == Erased::Unit
}

/// The constant literal `t` is, but `()` and `null`.
fn constant(l: &Lower, t: TNodeId) -> Option<Constant> {
    match l.tpd.tree(t) {
        Tree::Literal { value } if !matches!(value, Constant::Unit | Constant::Null) => Some(value.clone()),
        _ => None,
    }
}

/// Literal `value` of its constant type at `span`.
fn literal(l: &mut Lower, value: Constant, span: Span) -> TNodeId {
    let tpe = typer::tpd::constant_type_of(l.u, &value);
    l.tpd.node(Tree::Literal { value }, span, Some(tpe), None)
}
