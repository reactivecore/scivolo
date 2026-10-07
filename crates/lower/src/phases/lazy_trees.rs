//! The trees of lazy values' methods, each positioned when made, as dotty's `Positioned` does.

use super::lazy_vals::Lazy;
use crate::Lower;
use crate::definitions::{symbol, type_ref};
use syntax::ast::Span;
use tasty::tags::MUTABLE;
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

pub fn object(l: &mut Lower) -> TypeId {
    type_ref(l.u, l.defs.object)
}

pub fn unit(l: &mut Lower) -> TypeId {
    l.u.intern_type(Type::Constant(tasty::tags::UNITconst, 0))
}

/// A local value `name` of `owner` of type `info`, a variable if `mutable`.
pub fn local(l: &mut Lower, owner: SymbolId, name: &str, info: TypeId, mutable: bool) -> SymbolId {
    let flags = if mutable { Flags::default().with(MUTABLE) } else { Flags::default() };
    let s = l.u.define(symbol(name, Kind::Val, owner, flags));
    l.u.set_info(s, info);
    s
}

/// `this.x$lzyN`.
pub fn field_ref(l: &mut Lower, lazy: &Lazy) -> TNodeId {
    let this = l.this(lazy.class, Span::NONE);
    let select = l.select(this, lazy.field, Span::NONE);
    l.positioned(select)
}

/// `qual.m(args)`, of `m`'s result type, of `qual`'s type for a constructor.
pub fn call(l: &mut Lower, qual: TNodeId, m: SymbolId, args: &[TNodeId]) -> TNodeId {
    let fun = l.select(qual, m, Span::NONE);
    let fun = l.positioned(fun);
    let info = l.u.info(m);
    let result = match l.u.types().get(info).clone() {
        _ if l.u.symbol(m).name == "<init>" => l.tpd.tpe(qual),
        Type::Method { result, .. } => result,
        _ => info,
    };
    let apply = l.apply(fun, args, result, Span::NONE);
    l.positioned(apply)
}

/// `a eq b`.
pub fn eq(l: &mut Lower, a: TNodeId, b: TNodeId) -> TNodeId {
    let eq = l.defs.lazy_vals.eq;
    call(l, a, eq, &[b])
}

pub fn if_(l: &mut Lower, cond: TNodeId, thenp: TNodeId, elsep: TNodeId, tpe: TypeId) -> TNodeId {
    let t = l.tpd.node(Tree::If { cond, thenp, elsep }, Span::NONE, Some(tpe), None);
    l.positioned(t)
}

pub fn block(l: &mut Lower, stats: &[TNodeId], expr: TNodeId) -> TNodeId {
    let b = l.block(stats, expr, Span::NONE);
    l.positioned(b)
}

pub fn val_def(l: &mut Lower, s: SymbolId, rhs: TNodeId) -> TNodeId {
    let name = l.term_name(&l.u.symbol(s).name.clone());
    let info = l.u.info(s);
    let tpt = l.type_tree(info, Span::NONE);
    let def = l.tpd.node(Tree::ValDef { name, tpt, rhs: Some(rhs) }, Span::NONE, Some(info), Some(s));
    l.positioned(def)
}

pub fn assign(l: &mut Lower, var: SymbolId, rhs: TNodeId) -> TNodeId {
    let lhs = l.ident(var, Span::NONE);
    let unit = unit(l);
    let t = l.tpd.node(Tree::Assign { lhs, rhs }, Span::NONE, Some(unit), None);
    l.positioned(t)
}

/// `return expr` from method `from`.
pub fn return_(l: &mut Lower, from: SymbolId, expr: TNodeId) -> TNodeId {
    let from = l.ident(from, Span::NONE);
    let nothing = l.u.builtins().nothing;
    let nothing = type_ref(l.u, nothing);
    let t = l.tpd.node(Tree::Return { expr, from }, Span::NONE, Some(nothing), None);
    l.positioned(t)
}
