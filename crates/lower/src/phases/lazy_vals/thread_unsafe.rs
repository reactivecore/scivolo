// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LazyVals.scala
//! The `@threadUnsafe` lazy values of a class (dotty's `transformMemberDefThreadUnsafe`): a field
//! of the value's type, a flag `xbitmap$N` unless the type excludes `null`, and an accessor
//! computing the value when the flag is unset.

use super::{Lazy, fresh};
use crate::definitions::symbol;
use crate::phases::lazy_trees::{block, call, dup, eq, field_ref, if_, unit};
use crate::symbols::has;
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::tags::{LAZY, MUTABLE, PRIVATE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The field, the flag if any and the accessor definition `stat` of lazy value `lazy`, the
/// accessor computing `rhs`.
pub fn members(l: &mut Lower, lazy: &Lazy, stat: TNodeId, rhs: TNodeId) -> Result<Vec<TNodeId>> {
    let span = l.tpd.span(stat);
    let info = l.u.info(lazy.accessor);
    let tpe = value_type(l, info);
    l.u.set_info(lazy.field, tpe);
    let field_tpt = l.type_tree(tpe, Span::NONE);
    let mut out = vec![l.build().val_def(lazy.field, field_tpt, None, span)];
    let flag = flag(l, lazy, tpe);
    if let Some(flag) = flag {
        let boolean = l.u.info(flag);
        let flag_tpt = l.type_tree(boolean, Span::NONE);
        out.push(l.build().val_def(flag, flag_tpt, None, Span::NONE));
    }
    let not = l.lazy_vals()?.not;
    let rhs_span = l.tpd.span(rhs);
    let at = if rhs_span.exists() { rhs_span.with_point(rhs_span.start()) } else { span };
    let body = accessor_body(l, lazy, (flag, not), rhs, tpe);
    let Tree::DefDef { name, paramss, tpt, .. } = l.tpd.tree(stat).clone() else {
        unreachable!("an accessor")
    };
    let node = l.tpd.get_mut(stat);
    node.tree = Tree::DefDef { name, paramss, tpt, rhs: Some(body) };
    node.span = Span::NONE;
    out.push(l.with_span(stat, at));
    Ok(out)
}

/// Whether lazy value `accessor` is annotated `@threadUnsafe`, or the trait's it is taken from.
pub fn is_thread_unsafe(l: &mut Lower, accessor: SymbolId) -> bool {
    let annotated = |l: &mut Lower, s| l.u.is_annotated(s, "scala.annotation.threadUnsafe");
    if !l.mixed_in.contains(&accessor) {
        return annotated(l, accessor);
    }
    let (Some(class), name) = (l.u.symbol(accessor).owner, l.u.symbol(accessor).name.clone()) else {
        return false;
    };
    let bases: Vec<SymbolId> =
        l.u.linearization(class).into_iter().skip(1).filter(|&b| l.u.is_trait(b)).collect();
    let getters: Vec<SymbolId> =
        bases.into_iter().flat_map(|b| l.u.decl(b, &name).unwrap_or_default()).collect();
    getters.into_iter().any(|m| has(l.u, m, LAZY) && annotated(l, m))
}

/// The flag of lazy value `lazy` of type `tpe`, defined in its class, unless an object's type
/// excludes `null`, which then marks the field unset.
pub fn flag(l: &mut Lower, lazy: &Lazy, tpe: TypeId) -> Option<SymbolId> {
    if l.u.class_symbol(tpe).is_some_and(|c| l.u.is_module_class(c)) {
        return None;
    }
    let name = fresh(l, &format!("{}bitmap$", l.u.symbol(lazy.accessor).name));
    let flags = Flags::default().with(SYNTHETIC).with(MUTABLE).with(PRIVATE);
    let flag = l.u.define(symbol(&name, Kind::Val, lazy.class, flags));
    let boolean = l.u.class_ref("scala.Boolean");
    l.u.set_info(flag, boolean);
    l.fields.insert(flag, flag);
    Some(flag)
}

/// `{ if !flag then { x$lzy = rhs; flag = true }; x$lzy }`, or with `x$lzy eq null` for the
/// test and no flag (dotty's `mkThreadUnsafeDef`, `mkDefThreadUnsafeNonNullable`).
pub fn accessor_body(
    l: &mut Lower,
    lazy: &Lazy,
    (flag, not): (Option<SymbolId>, SymbolId),
    rhs: TNodeId,
    tpe: TypeId,
) -> TNodeId {
    let target = field_ref(l, lazy);
    let store = assign(l, target, rhs);
    let (cond, set, shared) = match flag {
        Some(flag) => {
            let flag_ref = select_field(l, lazy.class, flag);
            let truth = l.boolean_literal(true);
            let set = assign(l, flag_ref, truth);
            let not = call(l, flag_ref, not, &[]);
            (not, block(l, &[store], set), set)
        }
        None => {
            let null = l.null_literal(Span::NONE);
            (eq(l, target, null), store, store)
        }
    };
    let unit_tpe = unit(l);
    let otherwise = l.unit_literal(Span::NONE);
    let init = if_(l, cond, set, otherwise, unit_tpe);
    let body = block(l, &[init], target);
    unshare(l, body, shared, tpe);
    body
}

/// Gives the uses of the trees `accessor_body` shares, as dotty does, copies of their own: the
/// read of the field in `body`, of type `tpe`, and the left-hand side of assignment `set`.
fn unshare(l: &mut Lower, body: TNodeId, set: TNodeId, tpe: TypeId) {
    if let Tree::Block { stats, expr } = *l.tpd.tree(body) {
        let read = dup(l, expr);
        l.tpd.get_mut(read).tpe = Some(tpe);
        l.tpd.get_mut(body).tree = Tree::Block { stats, expr: read };
    }
    if let Tree::Assign { lhs, rhs } = *l.tpd.tree(set) {
        let lhs = dup(l, lhs);
        l.tpd.get_mut(set).tree = Tree::Assign { lhs, rhs };
    }
}

/// `this.field`, unpositioned.
fn select_field(l: &mut Lower, class: SymbolId, field: SymbolId) -> TNodeId {
    let this = l.this(class, Span::NONE);
    l.select(this, field, Span::NONE)
}

fn assign(l: &mut Lower, lhs: TNodeId, rhs: TNodeId) -> TNodeId {
    let unit = unit(l);
    let t = l.tpd.node(Tree::Assign { lhs, rhs }, Span::NONE, Some(unit), None);
    l.positioned(t)
}

/// The type of the value lazy `accessor` of type `info` holds.
pub fn value_type(l: &mut Lower, info: TypeId) -> TypeId {
    match *l.u.types().get(info) {
        Type::ByName(r) => r,
        _ => l.u.result_type(info),
    }
}
