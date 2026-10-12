//! dotty's `LazyVals` (`transformMemberDefThreadSafeNew`): each lazy value of a class becomes a
//! volatile field, an accessor reading it and a `$lzyINIT` method computing it under
//! compare-and-set of the field (Scala Native's `LazyVals.objCAS`); those `Mixin` adds come
//! after the class's own.

mod accessor;

use super::lazy_init::init_method;
use super::lazy_trees::object;
use crate::definitions::symbol;
use crate::symbols::has;
use crate::{Lower, Result};
use accessor::accessor_body;
use syntax::ast::Span;
use tasty::tags::{LAZY, MUTABLE, PRIVATE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type};
use typer::tpd::{TNodeId, Tree};

/// A lazy value's field: its class, the field, its accessor and the accessor's definition span
/// before `LazyVals`.
pub struct LazyField {
    pub class: SymbolId,
    pub field: SymbolId,
    pub accessor: SymbolId,
    pub span: Span,
}

/// The lazy value being made: its class, accessor, field and `$lzyINIT` method.
pub struct Lazy {
    pub class: SymbolId,
    pub accessor: SymbolId,
    pub field: SymbolId,
    pub init: SymbolId,
}

pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return Ok(()) };
    let mut stats = Vec::new();
    let mut mixed_in = Vec::new();
    for stat in l.tpd.items(body).to_vec() {
        let lazy = l.tpd.sym(stat).filter(|&s| has(l.u, s, LAZY));
        let (Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) }, Some(accessor)) =
            (l.tpd.tree(stat).clone(), lazy)
        else {
            stats.push(stat);
            continue;
        };
        let span = l.tpd.span(stat);
        if l.u.is_trait(class) {
            stats.push(stat);
            continue;
        }
        if is_eager_module(l, accessor) {
            stats.extend(eager_module(l, accessor, stat, rhs));
            continue;
        }
        let info = l.u.info(accessor);
        let lazy = define(l, class, accessor);
        let object = object(l);
        let field_tpt = l.type_tree(object, Span::NONE);
        let field_span = if l.mixed_in.contains(&accessor) { Span::NONE } else { span };
        stats.push(l.build().val_def(lazy.field, field_tpt, None, field_span));
        let rhs_span = l.tpd.span(rhs);
        let at = if rhs_span.exists() { rhs_span.with_point(rhs_span.start()) } else { span };
        let body = accessor_body(l, &lazy, info);
        let node = l.tpd.get_mut(stat);
        node.tree = Tree::DefDef { name, paramss, tpt, rhs: Some(body) };
        node.span = Span::NONE;
        stats.push(l.with_span(stat, at));
        stats.push(init_method(l, &lazy, rhs, info));
        let field = LazyField { class, field: lazy.field, accessor, span };
        match l.mixed_in.contains(&accessor) {
            true => mixed_in.push(field),
            false => l.lazy_fields.push(field),
        }
    }
    l.lazy_fields.extend(mixed_in);
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
    Ok(())
}

/// The `$lzyINIT` method and the field of a lazy value of a class, the field entered into the
/// class by `transform`.
#[derive(Clone, Copy)]
pub struct LazyMembers {
    pub init: SymbolId,
    pub field: SymbolId,
}

/// Defines the members of lazy value `accessor` of `class`, unnamed until `name_members`, and
/// makes the definitions in its right-hand side `rhs` owned by the `$lzyINIT` method, as
/// `LambdaLift` finds them in dotty.
pub fn define_members(l: &mut Lower, class: SymbolId, accessor: SymbolId, rhs: TNodeId) {
    let init = l.u.define(symbol("", Kind::Def, class, Flags::default().with(PRIVATE).with(SYNTHETIC)));
    let binder = l.u.fresh_binder();
    let object = object(l);
    let params = Vec::new();
    let info =
        l.u.intern_type(Type::Method { binder, params, result: object, implicit: false, contextual: false });
    l.u.set_info(init, info);
    let field = l.u.define(symbol("", Kind::Val, class, Flags::default().with(PRIVATE).with(MUTABLE)));
    l.u.unenter(field);
    l.u.set_info(field, object);
    l.lazy_members.insert(accessor, LazyMembers { init, field });
    for s in super::walk::subtrees(l.tpd, rhs) {
        let defined = matches!(l.tpd.tree(s), Tree::ValDef { .. } | Tree::DefDef { .. });
        if let Some(sym) = l.tpd.sym(s).filter(|&sym| defined && l.u.symbol(sym).owner == Some(accessor)) {
            l.u.set_owner(sym, init);
        }
    }
}

/// Names the field `x$lzyN` and the method `x$lzyINITN` of lazy value `accessor` of a class, in
/// that order, the field only for an eager object.
pub fn name_members(l: &mut Lower, accessor: SymbolId) {
    let name = l.u.symbol(accessor).name.clone();
    let members = l.lazy_members[&accessor];
    let field_name = fresh(l, &format!("{name}$lzy"));
    l.u.rename(members.field, &field_name);
    if is_eager_module(l, accessor) {
        return;
    }
    let init_name = fresh(l, &format!("{name}$lzyINIT"));
    l.u.rename(members.init, &init_name);
}

/// Whether `accessor` is the value of a synthetic object of a class that it does not take from a
/// trait, which dotty's `transformSyntheticModule` makes a field set in the constructor.
fn is_eager_module(l: &Lower, accessor: SymbolId) -> bool {
    let class = l.u.symbol(accessor).owner.filter(|&o| l.u.is_class(o) && !l.u.is_trait(o));
    class.is_some()
        && l.u.is_module(accessor)
        && has(l.u, accessor, SYNTHETIC)
        && !l.mixed_in.contains(&accessor)
}

/// The field `x$lzyN` of eager object `accessor` of definition `stat`, initialized with `rhs`,
/// and `stat` reading it.
fn eager_module(l: &mut Lower, accessor: SymbolId, stat: TNodeId, rhs: TNodeId) -> [TNodeId; 2] {
    let field = l.lazy_members[&accessor].field;
    let info = l.u.info(accessor);
    let tpe = l.u.result_type(info);
    l.u.set_info(field, tpe);
    l.u.set_flags(field, Flags::default().with(SYNTHETIC));
    l.u.enter(field);
    l.fields.insert(field, field);
    let at = l.tpd.span(stat).start_pos();
    let tpt = l.type_tree(tpe, Span::NONE);
    let def = l.build().val_def(field, tpt, Some(rhs), Span::NONE);
    let read = l.field_ref(field, at);
    let Tree::DefDef { name, paramss, tpt, .. } = l.tpd.tree(stat).clone() else { unreachable!("a getter") };
    let node = l.tpd.get_mut(stat);
    (node.tree, node.span) = (Tree::DefDef { name, paramss, tpt, rhs: Some(read) }, at);
    [def, stat]
}

/// The lazy value `accessor` of `class` with its field entered.
fn define(l: &mut Lower, class: SymbolId, accessor: SymbolId) -> Lazy {
    let LazyMembers { init, field } = l.lazy_members[&accessor];
    l.u.enter(field);
    l.fields.insert(field, field);
    Lazy { class, accessor, field, init }
}

/// `base` followed by the number of names made from it in the unit.
pub fn fresh(l: &mut Lower, base: &str) -> String {
    let name = l.fresh.next(l.names, base);
    l.names.text(name).to_owned()
}
