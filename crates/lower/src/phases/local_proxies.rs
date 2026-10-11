//! The fields and constructor parameters standing for the values local classes use, and the
//! order in which the local classes are named.

use super::lift_names::{Pending, Proxy};
use super::local_free::Local;
use crate::Lower;
use crate::definitions::symbol;
use tasty::tags::{LOCAL, PRIVATE};
use tasty::{Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The fields and constructor parameters of each local class of `locals`, put in `pending` to
/// name.
pub fn make_proxies(
    l: &mut Lower,
    locals: &[Local],
    pending: &mut Pending,
) -> Vec<(Vec<SymbolId>, Vec<SymbolId>)> {
    let mut out = Vec::new();
    for c in locals {
        let flags = Flags::default().with(PRIVATE).with(LOCAL);
        let fs: Vec<SymbolId> =
            c.free.in_methods.iter().map(|&v| proxy(l, v, c.class, Kind::Val, flags)).collect();
        let ctor = constructor(l, c.td);
        let ps: Vec<SymbolId> =
            c.free.values.iter().map(|&v| proxy(l, v, ctor, Kind::Param, Flags::default())).collect();
        let fields =
            c.free.in_methods.iter().zip(&fs).map(|(&value, &sym)| Proxy { owner: c.class, value, sym });
        let params = c.free.values.iter().zip(&ps).map(|(&value, &sym)| Proxy { owner: ctor, value, sym });
        pending.proxies.extend(fields.chain(params));
        out.push((fs, ps));
    }
    out
}

/// The symbol of the constructor of class definition `td`.
pub fn constructor(l: &Lower, td: TNodeId) -> SymbolId {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, .. } = *l.tpd.tree(template) else { unreachable!("a template") };
    l.tpd.sym(constr).expect("a constructor's symbol")
}

/// A symbol of `owner` standing for value `v`, named as `v` until named `v$N`.
fn proxy(l: &mut Lower, v: SymbolId, owner: SymbolId, kind: Kind, flags: Flags) -> SymbolId {
    let name = l.u.symbol(v).name.clone();
    let p = l.u.define(symbol(&name, kind, owner, flags));
    let info = l.u.info(v);
    l.u.set_info(p, info);
    p
}

/// The indices of `locals` in the order of their definitions, an object's class after its
/// companion class if that follows it (as dotty's `Namer` enters them).
pub fn definition_order(l: &Lower, locals: &[Local]) -> Vec<usize> {
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
fn companions(l: &Lower, locals: &[Local]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (i, local) in locals.iter().enumerate().filter(|(_, c)| l.u.is_module_class(c.class)) {
        let m = local.class;
        let Some(name) = tasty::module_name(&l.u.symbol(m).name) else { continue };
        let owner = l.u.symbol(m).owner;
        let class = (i + 1..locals.len()).find(|&j| {
            let c = locals[j].class;
            !l.u.is_module_class(c) && l.u.symbol(c).name == name && l.u.symbol(c).owner == owner
        });
        out.extend(class.map(|j| (i, j)));
    }
    out
}
