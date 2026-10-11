//! The fields and constructor parameters standing for the values local classes use, in the order
//! dotty's `LambdaLift` names them.

use super::class_deps::Added;
use super::lift_names::{Pending, Proxies};
use super::local_free::Free;
use crate::Lower;
use crate::definitions::symbol;
use tasty::tags::{LOCAL, PRIVATE};
use tasty::{Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The fields and constructor parameters of each local class of `locals`, put in `pending` in
/// naming order: those each class uses itself, its parameters then its fields, then the ones of
/// `added` in order.
pub fn make_proxies(
    l: &mut Lower,
    locals: &[(TNodeId, SymbolId, SymbolId, Free)],
    added: &[Added],
    pending: &mut Pending,
) -> Vec<(Vec<SymbolId>, Vec<SymbolId>)> {
    let mut out = Vec::new();
    for &(td, class, _, ref free) in locals {
        let flags = Flags::default().with(PRIVATE).with(LOCAL);
        let fs: Vec<SymbolId> =
            free.in_methods.iter().map(|&v| proxy(l, v, class, Kind::Val, flags)).collect();
        let ctor = constructor(l, td);
        let ps: Vec<SymbolId> =
            free.values.iter().map(|&v| proxy(l, v, ctor, Kind::Param, Flags::default())).collect();
        let direct = |syms: &[SymbolId], rounds: &[usize]| -> Vec<SymbolId> {
            syms.iter().zip(rounds).filter(|&(_, &r)| r == 0).map(|(&s, _)| s).collect::<Vec<_>>()
        };
        let syms: Vec<SymbolId> =
            direct(&ps, &free.rounds).into_iter().chain(direct(&fs, &free.field_rounds)).collect();
        if !syms.is_empty() {
            pending.proxies.push(Proxies { at: l.tpd.span(td).start(), round: 0, syms });
        }
        out.push((fs, ps));
    }
    for &(i, field, v, round) in added {
        let free = &locals[i].3;
        let (values, syms) = if field { (&free.in_methods, &out[i].0) } else { (&free.values, &out[i].1) };
        let k = values.iter().position(|&w| w == v).expect("an added value");
        pending.proxies.push(Proxies { at: l.tpd.span(locals[i].0).start(), round, syms: vec![syms[k]] });
    }
    out
}

/// The symbol of the constructor of class definition `td`.
fn constructor(l: &Lower, td: TNodeId) -> SymbolId {
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
