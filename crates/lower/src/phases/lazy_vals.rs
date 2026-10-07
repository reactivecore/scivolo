//! dotty's `LazyVals` (`transformMemberDefThreadSafeNew`): each lazy value of a class becomes a
//! volatile field, an accessor reading it and a `$lzyINIT` method computing it under
//! compare-and-set of the field (Scala Native's `LazyVals.objCAS`).

use super::adapt::adapt;
use super::lazy_init::init_method;
use super::lazy_trees::{block, call, eq, field_ref, if_, local, object, val_def};
use crate::definitions::symbol;
use crate::symbols::{has, is_trait};
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

/// The field `x$lzyN` and method `x$lzyINITN` of lazy value `accessor` of `class`.
fn define(l: &mut Lower, class: SymbolId, accessor: SymbolId) -> Lazy {
    let name = l.u.symbol(accessor).name.clone();
    let field_name = fresh(l, &format!("{name}$lzy"));
    let field =
        l.u.define(symbol(&field_name, Kind::Val, class, Flags::default().with(PRIVATE).with(MUTABLE)));
    let object = object(l);
    l.u.set_info(field, object);
    l.fields.insert(field, field);
    let init_name = fresh(l, &format!("{name}$lzyINIT"));
    let init =
        l.u.define(symbol(&init_name, Kind::Def, class, Flags::default().with(PRIVATE).with(SYNTHETIC)));
    let binder = l.u.fresh_binder();
    let info = l.u.intern_type(Type::Method {
        binder,
        params: Vec::new(),
        result: object,
        implicit: false,
        contextual: false,
    });
    l.u.set_info(init, info);
    Lazy { class, accessor, field, init }
}

/// `base` followed by the number of names made from it in the unit.
fn fresh(l: &mut Lower, base: &str) -> String {
    let name = l.fresh.next(l.names, base);
    l.names.text(name).to_owned()
}

/// `{ val result = x$lzy; if result.isInstanceOf[T] then result else if result eq NullValue then
/// null else x$lzyINIT() }`, each branch adapted to the value's type `info`.
fn accessor_body(l: &mut Lower, lazy: &Lazy, info: TypeId) -> TNodeId {
    let object = object(l);
    let result = local(l, lazy.accessor, "result", object, false);
    let field = field_ref(l, lazy);
    let def = val_def(l, result, field);
    let erased = l.u.erase(info);
    let test_type = boxed_type(l, &erased, info);
    let read = l.ident(result, Span::NONE);
    let cond = l.is_instance(read, test_type);
    let read = l.ident(result, Span::NONE);
    let value = adapt(l, read, &erased);
    let read = l.ident(result, Span::NONE);
    let null_value = l.ident(l.defs.lazy_vals.null_value, Span::NONE);
    let is_null = eq(l, read, null_value);
    let null = l.null_literal(Span::NONE);
    l.tpd.get_mut(null).tpe = Some(object);
    let null = adapt(l, null, &erased);
    let this = l.this(lazy.class, Span::NONE);
    let init = call(l, this, lazy.init, &[]);
    let init = adapt(l, init, &erased);
    let inner = if_(l, is_null, null, init, info);
    let outer = if_(l, cond, value, inner, info);
    block(l, &[def], outer)
}

/// The class `T` of type `info` erasing to `erased` as a reference: its box for a primitive.
fn boxed_type(l: &mut Lower, erased: &Erased, info: TypeId) -> TypeId {
    match erased {
        Erased::Prim(c) => l.u.class_ref(super::type_tests::boxed_class(*c).expect("a primitive's box")),
        _ => info,
    }
}
