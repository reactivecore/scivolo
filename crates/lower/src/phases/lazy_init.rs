// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/LazyVals.scala
// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/AdaptLazyVals.scala
//! The `$lzyINIT` method of a lazy value (dotty's `mkThreadSafeDef`): a loop reading the field
//! that computes the value after setting the field from `null` to `Evaluating`, waits while
//! another thread computes it, or returns it. Trees are made in dotty's order, sharing the trees
//! dotty shares.

use super::adapt::adapt;
use super::casts::cast;
use super::lazy_trees::{assign, block, call, eq, field_ref, if_, local, object, return_, unit, val_def};
use super::lazy_vals::Lazy;
use crate::Lower;
use crate::definitions::{LazyVals, type_ref};
use syntax::ast::Span;
use tasty::{Erased, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The trees every compare-and-set of one lazy value shares, and the symbols they refer to.
struct Shared {
    cas: TNodeId,
    field: TNodeId,
    evaluating: TNodeId,
    null_value: TNodeId,
    defs: LazyVals,
}

/// The `$lzyINIT` method of `lazy` computing `rhs` of type `info`.
pub fn init_method(l: &mut Lower, lazy: &Lazy, rhs: TNodeId, info: TypeId, defs: LazyVals) -> TNodeId {
    let native = l.ident(defs.native, Span::NONE);
    let cas = l.select(native, defs.obj_cas, Span::NONE);
    let field = field_ref(l, lazy);
    let evaluating = l.ident(defs.evaluating, Span::NONE);
    let null_value = l.ident(defs.null_value, Span::NONE);
    let s = Shared { cas, field, evaluating, null_value, defs };
    let (object, unit) = (object(l), unit(l));
    let result = local(l, lazy.init, "result", object, true);
    let nullable = local(l, lazy.init, "resultNullable", object, true);
    let release = release(l, lazy, &s, result);
    let evaluate = evaluate(l, &s, rhs, info, (result, nullable), release);
    let null = l.null_literal(Span::NONE);
    let start = apply_cas(l, &s, null, s.evaluating);
    let null = l.null_literal(Span::NONE);
    let result_def = val_def(l, result, null);
    let null = l.null_literal(Span::NONE);
    let nullable_def = val_def(l, nullable, null);
    let read = l.ident(nullable, Span::NONE);
    let ret = return_(l, lazy.init, read);
    let evaluate = block(l, &[result_def, nullable_def, evaluate], ret);
    let done = l.unit_literal(Span::NONE);
    let initialize = if_(l, start, evaluate, done, unit);
    let current = local(l, lazy.init, "current", object, false);
    let other = not_uninitialized(l, lazy, &s, current);
    let field = field_ref(l, lazy);
    let current_def = val_def(l, current, field);
    let read = l.ident(current, Span::NONE);
    let null = l.null_literal(Span::NONE);
    let uninitialized = eq(l, read, null);
    let step = if_(l, uninitialized, initialize, other, unit);
    let body = block(l, &[current_def], step);
    let cond = l.boolean_literal(true);
    let looped = l.tpd.node(Tree::WhileDo { cond, body }, Span::NONE, Some(unit), None);
    let looped = l.positioned(looped);
    l.endless_loops.insert(looped);
    let tpt = l.type_tree(object, Span::NONE);
    let def = l.method_def(lazy.init, &[], tpt, looped, Span::NONE);
    l.positioned(def)
}

/// `if !CAS(Evaluating, result) then { val lock = x$lzy.asInstanceOf[Waiting]; CAS(lock, result);
/// lock.countDown() }`.
fn release(l: &mut Lower, lazy: &Lazy, s: &Shared, result: SymbolId) -> TNodeId {
    let defs = s.defs;
    let waiting = type_ref(l.u, defs.waiting);
    let lock = local(l, lazy.init, "lock", waiting, false);
    let field = field_ref(l, lazy);
    let field = cast(l, field, &Erased::Class(l.u.binary_name(defs.waiting)));
    let field = l.positioned(field);
    let lock_def = val_def(l, lock, field);
    let lock_ref = l.ident(lock, Span::NONE);
    let read = l.ident(result, Span::NONE);
    let handover = apply_cas(l, s, lock_ref, read);
    let lock_ref = l.ident(lock, Span::NONE);
    let count_down = call(l, lock_ref, defs.count_down, &[]);
    let wake = block(l, &[lock_def, handover], count_down);
    let read = l.ident(result, Span::NONE);
    let swapped = apply_cas(l, s, s.evaluating, read);
    let failed = call(l, swapped, defs.not, &[]);
    let done = l.unit_literal(Span::NONE);
    let unit = unit(l);
    if_(l, failed, wake, done, unit)
}

/// `try { resultNullable = rhs; result = if resultNullable eq null then NullValue else
/// resultNullable } finally release`.
fn evaluate(
    l: &mut Lower,
    s: &Shared,
    rhs: TNodeId,
    info: TypeId,
    (result, nullable): (SymbolId, SymbolId),
    release: TNodeId,
) -> TNodeId {
    let value = match l.u.erase(info) {
        Erased::Prim(_) => {
            let boxed = adapt(l, rhs, &Erased::Class("java.lang.Object".into()));
            l.positioned(boxed)
        }
        _ => rhs,
    };
    let unit = unit(l);
    let compute = assign(l, nullable, value);
    let read = l.ident(nullable, Span::NONE);
    let null = l.null_literal(Span::NONE);
    let is_null = eq(l, read, null);
    let set_null = assign(l, result, s.null_value);
    let read = l.ident(nullable, Span::NONE);
    let set_value = assign(l, result, read);
    let store = if_(l, is_null, set_null, set_value, unit);
    let done = l.unit_literal(Span::NONE);
    let tried = block(l, &[compute, store], done);
    let cases = l.tpd.list(&[]);
    let attempt = Tree::Try { expr: tried, cases, finalizer: Some(release) };
    let attempt = l.tpd.node(attempt, Span::NONE, Some(unit), None);
    l.positioned(attempt)
}

/// `if current.isInstanceOf[LazyValControlState] then (if current eq Evaluating then
/// CAS(current, new Waiting) else if current.isInstanceOf[Waiting] then current.await() else
/// return null) else return current`.
fn not_uninitialized(l: &mut Lower, lazy: &Lazy, s: &Shared, current: SymbolId) -> TNodeId {
    let defs = s.defs;
    let (control, waiting, unit) = (type_ref(l.u, defs.control_state), type_ref(l.u, defs.waiting), unit(l));
    let read = l.ident(current, Span::NONE);
    let is_control = l.is_instance(read, control);
    let read = l.ident(current, Span::NONE);
    let is_evaluating = eq(l, read, s.evaluating);
    let read = l.ident(current, Span::NONE);
    let new = l.new_(defs.waiting, Span::NONE);
    let created = call(l, new, defs.waiting_ctor, &[]);
    let wait = apply_cas(l, s, read, created);
    let done = l.unit_literal(Span::NONE);
    let wait = block(l, &[wait], done);
    let read = l.ident(current, Span::NONE);
    let is_waiting = l.is_instance(read, waiting);
    let read = l.ident(current, Span::NONE);
    let lock = cast(l, read, &Erased::Class(l.u.binary_name(defs.waiting)));
    let lock = l.positioned(lock);
    let await_ = call(l, lock, defs.await_, &[]);
    let null = l.null_literal(Span::NONE);
    let give_up = return_(l, lazy.init, null);
    let waited = if_(l, is_waiting, await_, give_up, unit);
    let control_state = if_(l, is_evaluating, wait, waited, unit);
    let read = l.ident(current, Span::NONE);
    let ready = return_(l, lazy.init, read);
    if_(l, is_control, control_state, ready, unit)
}

/// `objCAS(this.x$lzy, expected, value)`.
fn apply_cas(l: &mut Lower, s: &Shared, expected: TNodeId, value: TNodeId) -> TNodeId {
    let boolean = l.u.class_ref("scala.Boolean");
    let apply = l.apply(s.cas, &[s.field, expected, value], boolean, Span::NONE);
    l.positioned(apply)
}
