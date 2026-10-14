//! Values of `scala.Enumeration` defined by `Value`, `Value(i)`, `Value(null)` or
//! `Value(i, null)`, given their field's name (Scala Native's `PrepNativeInterop`).

use super::walk::subtrees;
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Erased, Kind, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Names the values that class definition `td` defines by `Value` without a name.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    for t in subtrees(l.tpd, td) {
        let Tree::ValDef { name, tpt, rhs: Some(rhs) } = *l.tpd.tree(t) else { continue };
        let Some((value, index)) = unnamed_value(l, rhs) else { continue };
        let field = l.tpd.sym(t).expect("a value definition's symbol");
        let owner = l.u.symbol(field).owner.expect("a value's owner");
        if owner != class && l.u.symbol(owner).kind == Kind::Class {
            continue;
        }
        let enumeration = l.u.symbol(value).owner.expect("a method's owner");
        if owner != class || !l.u.derives_from(class, enumeration) {
            return unsupported(l.tpd.span(rhs), "Enumeration values outside their Enumeration");
        }
        let tpe = l.tpd.tpe(rhs);
        let default = l.u.symbol(field).name.clone();
        let named = named_value(l, enumeration, class, &default, index, tpe)?;
        l.tpd.get_mut(t).tree = Tree::ValDef { name, tpt, rhs: Some(named) };
        l.recopy(t);
    }
    Ok(())
}

/// `Value(index?, if (this.nextName != null && this.nextName.hasNext) this.nextName.next() else
/// default)` in class `class`, of type `tpe`.
fn named_value(
    l: &mut Lower,
    enumeration: SymbolId,
    class: SymbolId,
    default: &str,
    index: Option<TNodeId>,
    tpe: TypeId,
) -> Result<TNodeId> {
    let next_name = method(l, enumeration, "nextName", |_, _| true)?;
    let iterator = l.u.info(next_name);
    let [has_next, next] = ["hasNext", "next"].map(|n| l.u.member_of_type(iterator, n).first().copied());
    let (Some(has_next), Some(next)) = (has_next, next) else {
        return unsupported(Span::NONE, "Enumeration.nextName without hasNext and next");
    };
    let ne = method(l, l.defs.any, "!=", |_, _| true)?;
    let names = next_name_ref(l, class, next_name);
    let null = l.null_literal(Span::NONE);
    let not_null = call(l, names, ne, &[null]);
    let names = next_name_ref(l, class, next_name);
    let has_next = l.select(names, has_next, Span::NONE);
    let has_next = l.positioned(has_next);
    let cond = call(l, not_null, l.defs.and, &[has_next]);
    let names = next_name_ref(l, class, next_name);
    let thenp = call(l, names, next, &[]);
    let elsep = l.string_literal(default);
    let string = l.tpd.tpe(thenp);
    let name = l.tpd.node(Tree::If { cond, thenp, elsep, inline: false }, Span::NONE, Some(string), None);
    let name = l.positioned(name);
    let arity = 1 + usize::from(index.is_some());
    let value = method(l, enumeration, "Value", |l, m| {
        let info = l.u.info(m);
        let params = l.u.param_clauses(info).into_iter().next().map(|(_, ps)| ps).unwrap_or_default();
        params.len() == arity && params.last().is_some_and(|&p| l.u.erase(p) != Erased::Prim('i'))
    })?;
    let fun = l.ident(value, Span::NONE);
    let args: Vec<TNodeId> = index.into_iter().chain([name]).collect();
    let apply = l.apply(fun, &args, tpe, Span::NONE);
    Ok(l.positioned(apply))
}

/// `this.nextName` in class `class`.
fn next_name_ref(l: &mut Lower, class: SymbolId, next_name: SymbolId) -> TNodeId {
    let this = l.this(class, Span::NONE);
    let select = l.select(this, next_name, Span::NONE);
    l.positioned(select)
}

/// `qual.m(args)`, of `m`'s result type as a member of `qual`'s type.
fn call(l: &mut Lower, qual: TNodeId, m: SymbolId, args: &[TNodeId]) -> TNodeId {
    let fun = l.select(qual, m, Span::NONE);
    let fun = l.positioned(fun);
    let prefix = l.tpd.tpe(qual);
    let prefix = l.u.widen(prefix);
    let info = l.u.member_info(prefix, m);
    let result = l.u.result_type(info);
    let apply = l.apply(fun, args, result, Span::NONE);
    l.positioned(apply)
}

/// The term member of class `c` named `name` that `keep` accepts.
fn method(
    l: &mut Lower,
    c: SymbolId,
    name: &str,
    keep: impl Fn(&mut Lower, SymbolId) -> bool,
) -> Result<SymbolId> {
    let found = l.u.decl(c, name).unwrap_or_default();
    let found = found.into_iter().find(|&m| l.u.symbol(m).kind != Kind::Class && keep(l, m));
    found.map_or_else(|| unsupported(Span::NONE, format!("{} without {name}", l.u.full_name(c))), Ok)
}

/// `Value` of `Enumeration` and the index argument of `t` if `t` is `Value`, `Value(i)`,
/// `Value(null)` or `Value(i, null)`.
fn unnamed_value(l: &mut Lower, t: TNodeId) -> Option<(SymbolId, Option<TNodeId>)> {
    let (fun, args) = match *l.tpd.tree(t) {
        Tree::Apply { fun, args } => (fun, l.tpd.items(args).to_vec()),
        _ => (t, Vec::new()),
    };
    let m = l.tpd.sym(fun)?;
    let s = l.u.symbol(m);
    if s.kind != Kind::Def
        || s.name != "Value"
        || s.owner.is_none_or(|o| l.u.full_name(o) != "scala.Enumeration")
    {
        return None;
    }
    let null = |l: &Lower, a: TNodeId| matches!(l.tpd.tree(a), Tree::Literal { value: Constant::Null });
    let int = |l: &mut Lower, a: TNodeId| {
        let tpe = l.tpd.tpe(a);
        l.u.erase(tpe) == Erased::Prim('i')
    };
    let index = match args[..] {
        [] => None,
        [a] if null(l, a) => None,
        [a] if int(l, a) => Some(a),
        [a, b] if null(l, b) => Some(a),
        _ => return None,
    };
    Some((m, index))
}
