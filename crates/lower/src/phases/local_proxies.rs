// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LambdaLift.scala
//! The fields and constructor parameters standing for the values local classes use, and the
//! order in which the local classes are named.

use super::drop_inlined::naming_order;
use super::lift_names::{Pending, Proxy};
use super::local_free::{Local, is_term};
use crate::Lower;
use crate::definitions::symbol;
use crate::symbols::has;
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use syntax::source::Offset;
use tasty::tags::{FIELDaccessor, LAZY, LOCAL, PRIVATE};
use tasty::{Flags, Kind, SymbolId};
use typer::tpd::{TNodeId, Template, Tree};

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
    let Template { constr, .. } = l.tpd.class_template(td).expect("a class definition");
    l.tpd.sym(constr).expect("a constructor's symbol")
}

/// A symbol of `owner` standing for value `v`, named as `v` until named `v$N`.
pub fn proxy(l: &mut Lower, v: SymbolId, owner: SymbolId, kind: Kind, flags: Flags) -> SymbolId {
    let name = l.u.symbol(v).name.clone();
    let p = l.u.define(symbol(&name, kind, owner, flags));
    let info = l.u.info(v);
    l.u.set_info(p, info);
    p
}

/// The span of the definition of local value `v` in unit `root`, none for a parameter of an
/// `apply` that `SpecializeFunctions` copies with new parameters.
pub fn definition_span(l: &mut Lower, root: TNodeId, v: SymbolId) -> Span {
    let owner = l.u.symbol(v).owner;
    if l.u.symbol(v).kind == Kind::Param && owner.is_some_and(|m| super::specialize::is_specialized(l, m)) {
        return Span::NONE;
    }
    let def = super::walk::subtrees(l.tpd, root)
        .into_iter()
        .find(|&t| matches!(l.tpd.tree(t), Tree::ValDef { .. }) && l.tpd.sym(t) == Some(v));
    def.map_or(Span::NONE, |d| l.tpd.span(d))
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

/// The key `LambdaLift` names local class `class` of definition `td` by and its sequence: 1 in
/// the primary constructor of one of `classes` (a field's initializer or a statement of its
/// body), seen before that class's methods; 2 in the arguments of the parent constructor call of
/// one, seen after its body (`HoistSuperArgs`); else 0. In a local class of `named`, the key of that
/// class where it is in the constructor, or the class is.
pub fn naming_key(
    l: &Lower,
    (classes, named): (&[TNodeId], &FxHashMap<SymbolId, NamingKey>),
    td: TNodeId,
    class: SymbolId,
) -> NamingKey {
    let in_method = (naming_order(l, class, l.tpd.span(td).start()), 0);
    let mut s = class;
    let enclosing = loop {
        let Some(owner) = l.u.symbol(s).owner else { return in_method };
        if let Some((c, ctd)) = hoisted_from(l, classes, owner) {
            return (naming_order(l, c, l.tpd.span(ctd).end()), 2);
        }
        if !is_term(l, owner) {
            break owner;
        }
        s = owner;
    };
    let Some(&ctd) = classes.iter().find(|&&t| s != class && l.tpd.sym(t) == Some(enclosing)) else {
        return in_method;
    };
    let Some(Template { constr, .. }) = l.tpd.class_template(ctd) else { return in_method };
    let primary = l.tpd.sym(constr) == Some(s);
    let method = l.u.symbol(s).kind == Kind::Def && !primary && !has(l.u, s, FIELDaccessor);
    let outer = named.get(&enclosing).copied();
    if method || has(l.u, s, LAZY) {
        return outer.filter(|&(_, sequence)| sequence != 0).unwrap_or(in_method);
    }
    outer.unwrap_or((naming_order(l, enclosing, l.tpd.span(ctd).start()), 1))
}

/// The naming key of a local class and its sequence.
pub type NamingKey = ((Offset, Offset), usize);

/// The class among the definitions `classes` that method `m` hoists a superclass constructor
/// argument of, with its definition.
fn hoisted_from(l: &Lower, classes: &[TNodeId], m: SymbolId) -> Option<(SymbolId, TNodeId)> {
    let sym = l.u.symbol(m);
    let (name, _) = sym.name.split_once("$superArg$").filter(|_| sym.kind == Kind::Def)?;
    let hoisting = |c: SymbolId| {
        l.u.symbol(c).name == name && (Some(c) == sym.owner || l.u.symbol(c).owner == sym.owner)
    };
    classes.iter().find_map(|&t| l.tpd.sym(t).filter(|&c| hoisting(c)).map(|c| (c, t)))
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
