//! dotty's `LazyVals` (`transformMemberDefThreadSafeNew`): each lazy value of a class becomes a
//! volatile field, an accessor reading it and a `$lzyINIT` method computing it under
//! compare-and-set of the field (Scala Native's `LazyVals.objCAS`).

use super::adapt::adapt;
use super::lazy_init::init_method;
use super::lazy_trees::{block, call, eq, field_ref, if_, local, object, val_def};
use crate::definitions::symbol;
use crate::symbols::{has, is_singleton};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::{LAZY, MUTABLE, PRIVATE, SYNTHETIC};
use tasty::{Erased, Flags, Kind, SymbolId, Type, TypeId};
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
    for stat in l.tpd.items(body).to_vec() {
        let lazy = l.tpd.sym(stat).filter(|&s| has(l.u, s, LAZY));
        let (Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) }, Some(accessor)) =
            (l.tpd.tree(stat).clone(), lazy)
        else {
            stats.push(stat);
            continue;
        };
        let span = l.tpd.span(stat);
        if l.u.is_trait(class) && l.u.is_module(accessor) {
            stats.push(stat);
            continue;
        }
        if l.u.is_trait(class) {
            return unsupported(span, "lazy values in traits");
        }
        let info = l.u.info(accessor);
        if has(l.u, accessor, SYNTHETIC) && l.u.erase(info) == Erased::Unit {
            return unsupported(span, "lazy pattern definitions without variables");
        }
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
        l.lazy_fields.push(LazyField { class, field: lazy.field, accessor, span });
    }
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
/// that order.
pub fn name_members(l: &mut Lower, accessor: SymbolId) {
    let name = l.u.symbol(accessor).name.clone();
    let members = l.lazy_members[&accessor];
    let field_name = fresh(l, &format!("{name}$lzy"));
    l.u.rename(members.field, &field_name);
    let init_name = fresh(l, &format!("{name}$lzyINIT"));
    l.u.rename(members.init, &init_name);
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

/// `{ val result = x$lzy; if result.isInstanceOf[T] then result else if result eq NullValue then
/// null else x$lzyINIT() }`, each branch adapted to the value's type `info`; the test is
/// `result != null && !result.isInstanceOf[LazyValControlState]` where `T` includes the states.
fn accessor_body(l: &mut Lower, lazy: &Lazy, info: TypeId) -> TNodeId {
    let object = object(l);
    let result = local(l, lazy.accessor, "result", object, false);
    let field = field_ref(l, lazy);
    let def = val_def(l, result, field);
    let erased = l.u.erased_method(lazy.accessor).1;
    let info = if l.u.erase(info) == Erased::Unit { super::casts::type_of(l, &erased) } else { info };
    let cond = if holds_control_states(l, &erased) {
        let read = l.ident(result, Span::NONE);
        let null = l.null_literal(Span::NONE);
        let set = call(l, read, l.defs.patmat.ne, &[null]);
        let read = l.ident(result, Span::NONE);
        let control = crate::definitions::type_ref(l.u, l.defs.lazy_vals.control_state);
        let is_control = l.is_instance(read, control);
        let not_control = call(l, is_control, l.defs.lazy_vals.not, &[]);
        call(l, set, l.defs.patmat.and, &[not_control])
    } else {
        let test_type = boxed_type(l, &erased, info);
        let read = l.ident(result, Span::NONE);
        l.is_instance(read, test_type)
    };
    let read = l.ident(result, Span::NONE);
    let value = conform(l, read, &erased, info);
    let read = l.ident(result, Span::NONE);
    let null_value = l.ident(l.defs.lazy_vals.null_value, Span::NONE);
    let is_null = eq(l, read, null_value);
    let null = l.null_literal(Span::NONE);
    if erased != Erased::Null {
        l.tpd.get_mut(null).tpe = Some(object);
    }
    let null = conform(l, null, &erased, info);
    if is_singleton(l.u, info) {
        l.tpd.get_mut(null).tpe = Some(info);
    }
    let this = l.this(lazy.class, Span::NONE);
    let init = call(l, this, lazy.init, &[]);
    let init = conform(l, init, &erased, info);
    let inner = if_(l, is_null, null, init, info);
    let outer = if_(l, cond, value, inner, info);
    block(l, &[def], outer)
}

/// `t` adapted to erased type `erased` of type `tpe`, cast to `Null` or `Nothing` unless it is
/// `null` where `Null` is expected (dotty's `ensureConforms` after erasure).
fn conform(l: &mut Lower, t: TNodeId, erased: &Erased, tpe: TypeId) -> TNodeId {
    let null = matches!(l.tpd.tree(t), Tree::Literal { value: syntax::ast::constant::Constant::Null });
    match erased {
        Erased::Null if null => t,
        Erased::Null | Erased::Nothing => {
            let fun = l.select(t, l.defs.as_instance_of, Span::NONE);
            let tpt = l.type_tree(tpe, Span::NONE);
            let cast = l.build().type_apply(fun, &[tpt], tpe, Span::NONE);
            l.positioned(cast)
        }
        _ => adapt(l, t, erased),
    }
}

/// Whether the values of erased type `erased` include `LazyValControlState`'s.
fn holds_control_states(l: &mut Lower, erased: &Erased) -> bool {
    let Erased::Class(name) = erased else { return false };
    let control = l.defs.lazy_vals.control_state;
    super::casts::class_named(l, name).is_some_and(|c| l.u.derives_from(control, c))
}

/// The class `T` of type `info` erasing to `erased` as a reference: its box for a primitive.
fn boxed_type(l: &mut Lower, erased: &Erased, info: TypeId) -> TypeId {
    match erased {
        Erased::Prim(c) => l.u.class_ref(super::type_tests::boxed_class(*c).expect("a primitive's box")),
        _ => info,
    }
}
