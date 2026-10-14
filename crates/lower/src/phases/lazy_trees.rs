//! The trees of lazy values' methods, each positioned when made, as dotty's `Positioned` does.

use super::lazy_vals::Lazy;
use crate::definitions::{symbol, type_ref};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::tags::{MUTABLE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree, constant_type_of};

pub fn object(l: &mut Lower) -> TypeId {
    type_ref(l.u, l.defs.object)
}

pub fn unit(l: &mut Lower) -> TypeId {
    constant_type_of(l.u, &Constant::Unit)
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
    let eq = l.defs.eq;
    call(l, a, eq, &[b])
}

pub fn if_(l: &mut Lower, cond: TNodeId, thenp: TNodeId, elsep: TNodeId, tpe: TypeId) -> TNodeId {
    let t = l.tpd.node(Tree::If { cond, thenp, elsep, inline: false }, Span::NONE, Some(tpe), None);
    l.positioned(t)
}

pub fn block(l: &mut Lower, stats: &[TNodeId], expr: TNodeId) -> TNodeId {
    let b = l.block(stats, expr, Span::NONE);
    l.positioned(b)
}

pub fn val_def(l: &mut Lower, s: SymbolId, rhs: TNodeId) -> TNodeId {
    let name = l.build().term_name(s);
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

/// Method `name` of class `c`.
pub fn method(l: &mut Lower, c: SymbolId, name: &str, span: Span) -> Result<SymbolId> {
    let found = l.u.decl(c, name).unwrap_or_default();
    match found.into_iter().find(|&s| l.u.symbol(s).kind == Kind::Def) {
        Some(m) => Ok(m),
        None => unsupported(span, format!("{}.{name} missing", l.u.full_name(c))),
    }
}

/// A synthetic method `name()` of `owner` with result type `result`.
pub fn method_symbol(l: &mut Lower, name: &str, owner: SymbolId, result: TypeId) -> SymbolId {
    let s = l.u.define(symbol(name, Kind::Def, owner, Flags::default().with(SYNTHETIC)));
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method {
        binder,
        params: Vec::new(),
        result,
        implicit: false,
        contextual: false,
    });
    l.u.set_info(s, info);
    s
}

/// The definition of method `m()` with result type `result`.
pub fn method_def(l: &mut Lower, m: SymbolId, result: TypeId, rhs: TNodeId) -> TNodeId {
    let tpt = l.type_tree(result, Span::NONE);
    let def = l.method_def(m, &[], tpt, rhs, Span::NONE);
    l.positioned(def)
}

/// A copy of selection, application or reference `t` with its spans, for a tree dotty shares.
pub fn dup(l: &mut Lower, t: TNodeId) -> TNodeId {
    let tree = match *l.tpd.tree(t) {
        Tree::Ident { name } => Tree::Ident { name },
        Tree::Select { qualifier, name } => Tree::Select { qualifier: dup(l, qualifier), name },
        Tree::Apply { fun, args } => {
            let args: Vec<TNodeId> = l.tpd.items(args).to_vec();
            let args: Vec<TNodeId> = args.into_iter().map(|a| dup(l, a)).collect();
            Tree::Apply { fun: dup(l, fun), args: l.tpd.list(&args) }
        }
        ref other => unreachable!("a copied tree: {}", typer::tpd::print::class_name(other)),
    };
    let n = l.tpd.get(t);
    let (span, tpe, sym) = (n.span, n.tpe, n.sym);
    l.tpd.node(tree, span, tpe, sym)
}

/// `return expr` from method `from`.
pub fn return_(l: &mut Lower, from: SymbolId, expr: TNodeId) -> TNodeId {
    let from = l.ident(from, Span::NONE);
    let nothing = l.u.builtins().nothing;
    let nothing = type_ref(l.u, nothing);
    let t = l.tpd.node(Tree::Return { expr, from }, Span::NONE, Some(nothing), None);
    l.positioned(t)
}
