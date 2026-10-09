//! dotty's `LazyVals` (`transformMemberDefThreadSafeNew`): each lazy value of a class becomes a
//! volatile field, an accessor reading it and a `$lzyINIT` method computing it under
//! compare-and-set of the field (Scala Native's `LazyVals.objCAS`).

use super::adapt::adapt;
use super::lazy_init::init_method;
use super::lazy_trees::{block, call, eq, field_ref, if_, local, object, val_def};
use crate::definitions::symbol;
use crate::symbols::{has, is_singleton, is_trait};
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
        if is_trait(l.u, class) {
            return unsupported(span, "lazy values in traits");
        }
        let info = l.u.info(accessor);
        if matches!(l.u.erase(info), Erased::Unit | Erased::Null | Erased::Nothing) {
            return unsupported(span, "lazy values of type Unit, Null or Nothing");
        }
        let lazy = define(l, class, accessor);
        let object = object(l);
        let field_tpt = l.type_tree(object, Span::NONE);
        let field_name = l.term_name(&l.u.symbol(lazy.field).name.clone());
        let field_def = Tree::ValDef { name: field_name, tpt: field_tpt, rhs: None };
        stats.push(l.tpd.node(field_def, l.tpd.span(stat), None, Some(lazy.field)));
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

/// Defines the method `x$lzyINITN` of lazy value `accessor` of `class` and makes the
/// definitions in its right-hand side `rhs` owned by it, as `LambdaLift` finds them in dotty.
pub fn define_init(l: &mut Lower, class: SymbolId, accessor: SymbolId, rhs: TNodeId) {
    let name = l.u.symbol(accessor).name.clone();
    let init_name = fresh(l, &format!("{name}$lzyINIT"));
    let init =
        l.u.define(symbol(&init_name, Kind::Def, class, Flags::default().with(PRIVATE).with(SYNTHETIC)));
    let binder = l.u.fresh_binder();
    let result = object(l);
    let info = l.u.intern_type(Type::Method {
        binder,
        params: Vec::new(),
        result,
        implicit: false,
        contextual: false,
    });
    l.u.set_info(init, info);
    l.lazy_inits.insert(accessor, init);
    for s in super::walk::subtrees(l.tpd, rhs) {
        let defined = matches!(l.tpd.tree(s), Tree::ValDef { .. } | Tree::DefDef { .. });
        if let Some(sym) = l.tpd.sym(s).filter(|&sym| defined && l.u.symbol(sym).owner == Some(accessor)) {
            l.u.set_owner(sym, init);
        }
    }
}

/// The field `x$lzyN` and the method `define_init` made for lazy value `accessor` of `class`.
fn define(l: &mut Lower, class: SymbolId, accessor: SymbolId) -> Lazy {
    let name = l.u.symbol(accessor).name.clone();
    let field_name = fresh(l, &format!("{name}$lzy"));
    let field =
        l.u.define(symbol(&field_name, Kind::Val, class, Flags::default().with(PRIVATE).with(MUTABLE)));
    let object = object(l);
    l.u.set_info(field, object);
    l.fields.insert(field, field);
    let init = l.lazy_inits[&accessor];
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
    let erased = l.u.erase(info);
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
    let value = adapt(l, read, &erased);
    let read = l.ident(result, Span::NONE);
    let null_value = l.ident(l.defs.lazy_vals.null_value, Span::NONE);
    let is_null = eq(l, read, null_value);
    let null = l.null_literal(Span::NONE);
    l.tpd.get_mut(null).tpe = Some(object);
    let null = adapt(l, null, &erased);
    if is_singleton(l.u, info) {
        l.tpd.get_mut(null).tpe = Some(info);
    }
    let this = l.this(lazy.class, Span::NONE);
    let init = call(l, this, lazy.init, &[]);
    let init = adapt(l, init, &erased);
    let inner = if_(l, is_null, null, init, info);
    let outer = if_(l, cond, value, inner, info);
    block(l, &[def], outer)
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
