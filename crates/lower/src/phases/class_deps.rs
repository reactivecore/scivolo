//! The local values local classes use through their own fields, the local methods they call and
//! the local classes they create (dotty's `Dependencies.computeFreeVars` for classes), in the
//! order its passes find them.

use super::local_free::{Free, is_term, within};
use super::walk::subtrees;
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use tasty::tags::MUTABLE;
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// A value a pass adds to a local class: the class's index, whether to its fields rather than
/// its constructor's parameters, the value and the pass.
pub type Added = (usize, bool, SymbolId, usize);

/// Adds to the constructor parameters of each local class of `locals` in unit `root` the values
/// of its fields, of the local methods its constructor calls and of the constructors of the
/// classes it creates, and to its fields those its methods call or create, pass by pass, the
/// classes in definition order; what each pass added, in order.
pub fn close(
    l: &mut Lower,
    root: TNodeId,
    locals: &mut [(TNodeId, SymbolId, SymbolId, Free)],
) -> Result<Vec<Added>> {
    let mut functions: Vec<(SymbolId, Vec<SymbolId>)> = Vec::new();
    for (_, _, _, free) in locals.iter() {
        for &(f, _) in &free.called {
            if l.u.symbol(f).kind == Kind::Def && !functions.iter().any(|(g, _)| *g == f) {
                let mut values = Vec::new();
                function_values(l, root, f, &mut values, &mut vec![f])?;
                functions.push((f, values));
            }
        }
    }
    let order = definition_order(l, locals);
    let mut added = Vec::new();
    for pass in 1.. {
        let before = added.len();
        for &i in &order {
            let own = (locals[i].1, false);
            let called = std::iter::once(own).chain(locals[i].3.called.clone());
            let (ctor, methods): (Vec<_>, Vec<_>) = called.partition(|&(_, in_method)| !in_method);
            for (callee, in_method) in ctor.into_iter().chain(methods) {
                let values = match functions.iter().find(|(f, _)| *f == callee) {
                    Some((_, values)) => values.clone(),
                    None if callee == locals[i].1 => locals[i].3.in_methods.clone(),
                    None => match locals.iter().find(|c| c.1 == callee) {
                        Some((_, _, _, d)) => d.values.clone(),
                        None => continue,
                    },
                };
                let free = &mut locals[i].3;
                for v in values {
                    let (to, rounds) = match in_method {
                        true => (&mut free.in_methods, &mut free.field_rounds),
                        false => (&mut free.values, &mut free.rounds),
                    };
                    if !to.contains(&v) {
                        to.push(v);
                        rounds.push(pass);
                        added.push((i, in_method, v, pass));
                    }
                }
            }
        }
        if added.len() == before {
            break;
        }
    }
    Ok(added)
}

/// The indices of `locals` in the order of their definitions, an object's class after its
/// companion class if that follows it (as dotty's `Namer` enters them).
pub fn definition_order(l: &Lower, locals: &[(TNodeId, SymbolId, SymbolId, Free)]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..locals.len()).collect();
    for (m, c) in companions(l, locals) {
        let from = order.iter().position(|&i| i == m).expect("a local object");
        order.remove(from);
        let to = order.iter().position(|&i| i == c).expect("a local class");
        order.insert(to + 1, m);
    }
    order
}

/// The indices of the classes of the local objects among `locals` with a companion class after
/// them, each with the companion's.
fn companions(l: &Lower, locals: &[(TNodeId, SymbolId, SymbolId, Free)]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (i, &(_, m, _, _)) in locals.iter().enumerate().filter(|(_, c)| l.u.is_module_class(c.1)) {
        let Some(name) = tasty::module_name(&l.u.symbol(m).name) else { continue };
        let owner = l.u.symbol(m).owner;
        let class = (i + 1..locals.len()).find(|&j| {
            let c = locals[j].1;
            !l.u.is_module_class(c) && l.u.symbol(c).name == name && l.u.symbol(c).owner == owner
        });
        out.extend(class.map(|j| (i, j)));
    }
    out
}

/// Adds to `out` the local values local method `f` of unit `root` uses, then those of the local
/// methods it calls; `seen` the methods already visited.
fn function_values(
    l: &Lower,
    root: TNodeId,
    f: SymbolId,
    out: &mut Vec<SymbolId>,
    seen: &mut Vec<SymbolId>,
) -> Result<()> {
    let found = subtrees(l.tpd, root)
        .into_iter()
        .find(|&t| matches!(l.tpd.tree(t), Tree::DefDef { .. }) && l.tpd.sym(t) == Some(f));
    let Some(def) = found else { return Ok(()) };
    for t in subtrees(l.tpd, def) {
        let (Tree::Ident { .. }, Some(s)) = (l.tpd.tree(t), l.tpd.sym(t)) else { continue };
        if !l.u.symbol(s).owner.is_some_and(|o| is_term(l, o) && !within(l, o, f)) {
            continue;
        }
        match l.u.symbol(s).kind {
            Kind::Def if !seen.contains(&s) => {
                seen.push(s);
                function_values(l, root, s, out, seen)?;
            }
            Kind::Val | Kind::Param if !out.contains(&s) => {
                if has(l.u, s, MUTABLE) {
                    return unsupported(l.tpd.span(t), "local classes capturing variables");
                }
                out.push(s);
            }
            _ => {}
        }
    }
    Ok(())
}
