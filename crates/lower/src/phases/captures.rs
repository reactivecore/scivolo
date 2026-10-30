// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Dependencies.scala
//! What lifted functions capture (dotty's `Dependencies`): the local values free in them or in the
//! functions they call, in the order dotty records them, and whether they need `this`.

use super::lambda_lift::Function;
use super::lift_names::Pending;
use super::walk::{children, is_type_tree};
use crate::symbols::{has, is_static, is_static_owner};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use tasty::tags::MUTABLE;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// A step of the walk: a tree, or the definition of a lifted local class, with the values it uses.
enum Step<'p> {
    Tree(TNodeId),
    Class(&'p [SymbolId]),
}

/// What a function calls: another function, or a local class's constructor it passes a value.
#[derive(PartialEq)]
enum Callee {
    Function(usize),
    Value(SymbolId),
}

/// Sets the captured values and `this` of `functions` of `class`, whose methods have the bodies
/// `roots`, local classes lifted into `class` if `in_class`, those of `pending` using values where
/// they were defined and passed them where created.
pub fn dependencies(
    l: &Lower,
    functions: &mut [Function],
    class: SymbolId,
    roots: &[TNodeId],
    (in_class, pending): (bool, &Pending),
) -> Result<()> {
    let defs: FxHashMap<TNodeId, usize> = functions.iter().enumerate().map(|(i, f)| (f.def, i)).collect();
    let (mut order, mut called) = (Vec::new(), Vec::<(usize, Vec<Callee>)>::new());
    let mut todo: Vec<(Step, Option<usize>)> = roots.iter().rev().map(|&r| (Step::Tree(r), None)).collect();
    while let Some((step, enclosure)) = todo.pop() {
        let t = match step {
            Step::Tree(t) => t,
            Step::Class(values) => {
                if let Some(i) = enclosure {
                    for &s in values {
                        mark_free(l, functions, &mut order, s, i);
                    }
                }
                continue;
            }
        };
        let enclosure = defs.get(&t).copied().or(enclosure);
        let skipped = matches!(l.tpd.tree(t), Tree::TypeDef { .. }) || is_type_tree(l, t);
        if skipped {
            continue;
        }
        todo.extend(steps(l, t, pending).into_iter().rev().map(|s| (s, enclosure)));
        let Some(i) = enclosure else { continue };
        match (l.tpd.tree(t), l.tpd.sym(t)) {
            (Tree::This { .. }, s) => functions[i].this |= s.is_none() || s != functions[i].into,
            (&Tree::New { tpt }, _) if matches!(l.u.types().get(l.tpd.tpe(tpt)), Type::Sym(_, c) if l.local_classes.contains_key(c)) => {
                functions[i].this |= in_class
            }
            (&Tree::New { tpt }, _) => {
                let tpe = l.tpd.tpe(tpt);
                let tycon = if let Type::Applied(t, _) = l.u.types().get(tpe) { *t } else { tpe };
                let created = if let Type::Sym(_, c) = *l.u.types().get(tycon) { Some(c) } else { None };
                functions[i].this |= created.is_some_and(|c| l.u.symbol(c).owner == Some(class));
            }
            (Tree::Ident { .. }, Some(s)) if pending.passed.contains(&t) => {
                add_call(&mut called, i, Callee::Value(s))
            }
            (Tree::Ident { .. }, Some(s)) => {
                if let Some(j) = functions.iter().position(|f| f.sym == s) {
                    add_call(&mut called, i, Callee::Function(j));
                    continue;
                }
                let owner = l.u.symbol(s).owner;
                let implicit_this = implicit_this(l, l.tpd.get(t).tpe) == Some(class);
                functions[i].this |= (owner == Some(class) || implicit_this) && !l.u.is_module_class(class);
                let local = matches!(l.u.symbol(s).kind, Kind::Val | Kind::Param)
                    && owner.is_some_and(|o| matches!(l.u.symbol(o).kind, Kind::Def | Kind::Val));
                if local && mark_free(l, functions, &mut order, s, i) && has(l.u, s, MUTABLE) {
                    return unsupported(l.tpd.span(t), "local methods capturing variables");
                }
            }
            _ => {}
        }
    }
    loop {
        let mut changed = false;
        for (caller, callees) in &called {
            for callee in callees {
                let captured = match *callee {
                    Callee::Function(j) => {
                        changed |= functions[j].this && !functions[*caller].this;
                        functions[*caller].this |= functions[j].this;
                        let narrowed = functions[j].into.filter(|_| !functions[*caller].instance());
                        changed |= narrowed.is_some();
                        functions[*caller].into = functions[*caller].into.or(narrowed);
                        functions[j].captured.clone()
                    }
                    Callee::Value(s) => vec![s],
                };
                for s in captured {
                    changed |= mark_free(l, functions, &mut order, s, *caller);
                }
            }
        }
        if !changed {
            return Ok(());
        }
    }
}

/// The children of `t`, with the definitions of the local classes lifted out of it, if a block.
fn steps<'p>(l: &Lower, t: TNodeId, pending: &'p Pending) -> Vec<Step<'p>> {
    let mut steps: Vec<Step> = children(l.tpd, t).into_iter().map(Step::Tree).collect();
    let mut lifted: Vec<_> = pending.definitions.iter().rev().filter(|d| d.0 == t).collect();
    lifted.sort_by_key(|d| std::cmp::Reverse(d.1));
    for (_, index, values) in lifted {
        steps.insert(*index, Step::Class(values));
    }
    steps
}

/// The class whose `this` the prefix of reference type `tpe` starts with, through non-static
/// members (dotty's `captureImplicitThis`).
fn implicit_this(l: &Lower, tpe: Option<TypeId>) -> Option<SymbolId> {
    match *l.u.types().get(tpe?) {
        Type::Sym(prefix, s) if l.u.symbol(s).kind.is_term() && !is_static(l.u, s) => implicit_this(l, prefix),
        Type::This(c) => match *l.u.types().get(c) {
            Type::Sym(_, c) if !is_static_owner(l.u, c) => Some(c),
            _ => None,
        },
        _ => None,
    }
}

/// Records that function `caller` calls `callee`.
fn add_call(called: &mut Vec<(usize, Vec<Callee>)>, caller: usize, callee: Callee) {
    match called.iter_mut().find(|(c, _)| *c == caller) {
        Some((_, callees)) if !callees.contains(&callee) => callees.push(callee),
        Some(_) => {}
        None => called.push((caller, vec![callee])),
    }
}

/// Marks local value `s` free in function `i` and the functions it is in, up to the one defining
/// `s` or the value `s` is in, outermost first; whether that added any.
fn mark_free(l: &Lower, functions: &mut [Function], order: &mut Vec<usize>, s: SymbolId, i: usize) -> bool {
    let mut owner = l.u.symbol(s).owner;
    while let Some(o) =
        owner.filter(|&o| l.u.symbol(o).kind == Kind::Val && functions.iter().all(|f| f.sym != o))
    {
        owner = l.u.symbol(o).owner;
    }
    let mut chain = Vec::new();
    let mut k = Some(i);
    while let Some(j) = k.filter(|&j| Some(functions[j].sym) != owner) {
        chain.push(j);
        k = functions[j].parent;
    }
    if k.is_none() && functions.iter().any(|f| Some(f.sym) == owner) {
        return false;
    }
    let mut added = false;
    for &j in chain.iter().rev() {
        if functions[j].captured.contains(&s) {
            continue;
        }
        if functions[j].captured.is_empty() {
            order.push(j);
        }
        functions[j].captured.push(s);
        added = true;
    }
    added
}
