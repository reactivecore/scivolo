//! dotty's `LazyVals.transformLocalDef`: a local lazy value becomes a `LazyRef` holder at the
//! start of its block, a synchronized `$lzyINIT` method and an accessor the references call.

use super::casts::class_named;
use super::lazy_trees::{call, dup, if_, method, method_def, method_symbol, object, val_def};
use super::lazy_vals::fresh;
use super::walk::{children, subtrees};
use crate::definitions::{symbol, type_ref};
use crate::symbols::{has, is_module};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::tags::{LAZY, SYNTHETIC};
use tasty::{Erased, Flags, Kind, SymbolId, TypeId};
use typer::tpd::{TNodeId, Tree};

/// A local lazy value made methods: its holder's, its `$lzyINIT` method's and its accessor's
/// definitions, the value and its accessor.
struct Expanded {
    defs: [TNodeId; 3],
    value: SymbolId,
    accessor: SymbolId,
}

/// A holder: its symbol, its class and the type of its `value`.
struct Holder {
    sym: SymbolId,
    class: SymbolId,
    value: TypeId,
}

/// Makes the local lazy values of unit `root` methods and names the members of its classes' lazy
/// values, in one walk with inner trees first.
pub fn transform(l: &mut Lower, root: TNodeId) -> Result<()> {
    let mut expanded = FxHashMap::default();
    let mut todo = vec![(root, false)];
    while let Some((t, visited)) = todo.pop() {
        let lazy = l.tpd.sym(t).filter(|&s| has(l.u, s, LAZY) || is_module(l.u, s));
        if !visited {
            todo.push((t, true));
            todo.extend(children(l.tpd, t).into_iter().rev().map(|c| (c, false)));
        } else if let (Tree::ValDef { .. }, Some(s)) = (l.tpd.tree(t), lazy)
            && l.u.symbol(s).owner.is_some_and(|o| super::local_free::is_term(l, o))
        {
            expanded.insert(t, expand(l, t)?);
        } else if let (Tree::DefDef { .. }, Some(s)) = (l.tpd.tree(t), lazy)
            && l.lazy_members.contains_key(&s)
        {
            super::lazy_vals::name_members(l, s);
        } else if let Tree::Block { stats, expr } = *l.tpd.tree(t) {
            let items = l.tpd.items(stats).to_vec();
            if !items.iter().any(|s| expanded.contains_key(s)) {
                continue;
            }
            let (mut holders, mut others) = (Vec::new(), Vec::new());
            for s in items {
                match expanded.remove(&s) {
                    Some(e) => {
                        holders.push(e.defs[0]);
                        others.extend(&e.defs[1..]);
                        call_accessor(l, t, e.value, e.accessor);
                    }
                    None => others.push(s),
                }
            }
            holders.extend(others);
            let stats = l.tpd.list(&holders);
            l.tpd.get_mut(t).tree = Tree::Block { stats, expr };
            l.recopy(t);
        }
    }
    Ok(())
}

/// The holder, `$lzyINIT` method and accessor of local lazy value definition `stat`.
fn expand(l: &mut Lower, stat: TNodeId) -> Result<Expanded> {
    let span = l.tpd.span(stat);
    let value = l.tpd.sym(stat).expect("a value definition's symbol");
    let Tree::ValDef { rhs: Some(rhs), .. } = *l.tpd.tree(stat) else { unreachable!("a lazy value") };
    let owner = l.u.symbol(value).owner.expect("a local value's owner");
    let name = l.u.symbol(value).name.clone();
    let mut info = l.u.info(value);
    if l.u.erase(info) == Erased::Unit {
        let boxed_unit = l.u.symbol(l.defs.boxed_unit_unit).owner.expect("BoxedUnit");
        info = type_ref(l.u, boxed_unit);
    }
    let holder = holder(l, owner, &name, info, span)?;
    let new = l.new_(holder.class, Span::NONE);
    let ctor = method(l, holder.class, "<init>", span)?;
    let alloc = call(l, new, ctor, &[]);
    let holder_def = val_def(l, holder.sym, alloc);
    let holder_ref = l.ident(holder.sym, Span::NONE);
    let get_value = holder_call(l, &holder, holder_ref, "value", &[], span)?;
    let get_value = l.with_span(get_value, span);
    let receiver = dup(l, holder_ref);
    let initialized = holder_call(l, &holder, receiver, "initialized", &[], span)?;
    l.tpd.get_mut(initialized).tpe = Some(l.u.class_ref("scala.Boolean"));
    let init_name = fresh(l, &format!("{name}$lzyINIT"));
    let init = method_symbol(l, &init_name, owner, info);
    for s in subtrees(l.tpd, rhs) {
        if let Some(sym) = l.tpd.sym(s).filter(|&sym| l.u.symbol(sym).owner == Some(value)) {
            l.u.set_owner(sym, init);
        }
    }
    let receiver = dup(l, holder_ref);
    let initialize = holder_call(l, &holder, receiver, "initialize", &[rhs], span)?;
    let receiver = dup(l, holder_ref);
    let sync = l.select(receiver, l.defs.monitors.synchronized, Span::NONE);
    let sync = l.positioned(sync);
    let targ = l.type_tree(info, Span::NONE);
    let targs = l.tpd.list(&[targ]);
    let sync = l.tpd.node(Tree::TypeApply { fun: sync, args: targs }, Span::NONE, Some(info), None);
    let sync = l.positioned(sync);
    let cond = if_(l, initialized, get_value, initialize, holder.value);
    let body = conform(l, cond, &holder, info);
    let synced = l.apply(sync, &[body], info, Span::NONE);
    let synced = l.positioned(synced);
    let init_def = method_def(l, init, info, synced);
    let accessor = method_symbol(l, &name, owner, info);
    l.local_lazy_accessors.insert(accessor);
    let init_ref = l.ident(init, Span::NONE);
    let init_call = l.apply(init_ref, &[], info, Span::NONE);
    let init_call = l.positioned(init_call);
    let (initialized, get_value) = (dup(l, initialized), dup(l, get_value));
    let cond = if_(l, initialized, get_value, init_call, holder.value);
    let body = conform(l, cond, &holder, info);
    let accessor_def = method_def(l, accessor, info, body);
    l.position_thicket(&[holder_def, init_def, accessor_def]);
    Ok(Expanded { defs: [holder_def, init_def, accessor_def], value, accessor })
}

/// The holder `name$lzyN` of `owner` for a lazy value of type `info` (dotty's `LazyHolder`).
fn holder(l: &mut Lower, owner: SymbolId, name: &str, info: TypeId, span: Span) -> Result<Holder> {
    let (class_name, value) = match l.u.erase(info) {
        Erased::Prim(c) => {
            let prim = crate::definitions::primitive_class_name(c).expect("a primitive letter");
            (format!("scala.runtime.Lazy{}", &prim["scala.".len()..]), info)
        }
        _ => ("scala.runtime.LazyRef".to_owned(), object(l)),
    };
    let Some(class) = class_named(l, &class_name) else {
        return unsupported(span, format!("{class_name} missing"));
    };
    let holder_name = fresh(l, &format!("{name}$lzy"));
    let sym = l.u.define(symbol(&holder_name, Kind::Val, owner, Flags::default().with(SYNTHETIC)));
    let tpe = type_ref(l.u, class);
    l.u.set_info(sym, tpe);
    Ok(Holder { sym, class, value })
}

/// `receiver.m(args)` of the type of the holder's value.
fn holder_call(
    l: &mut Lower,
    holder: &Holder,
    receiver: TNodeId,
    m: &str,
    args: &[TNodeId],
    span: Span,
) -> Result<TNodeId> {
    let m = method(l, holder.class, m, span)?;
    let t = call(l, receiver, m, args);
    l.tpd.get_mut(t).tpe = Some(holder.value);
    Ok(t)
}

/// `t`, cast to `info` if the holder's value is a reference that `info` does not erase to
/// (dotty's `ensureConforms`).
fn conform(l: &mut Lower, t: TNodeId, holder: &Holder, info: TypeId) -> TNodeId {
    let erased_object = Erased::Class("java.lang.Object".into());
    if holder.value != object(l) || l.u.erase(info) == erased_object {
        return t;
    }
    let fun = l.select(t, l.defs.as_instance_of, Span::NONE);
    let fun = l.positioned(fun);
    let tpt = l.type_tree(info, Span::NONE);
    let args = l.tpd.list(&[tpt]);
    let cast = l.tpd.node(Tree::TypeApply { fun, args }, Span::NONE, Some(info), None);
    l.positioned(cast)
}

/// Makes the references to local lazy value `value` in `block` calls of its accessor.
fn call_accessor(l: &mut Lower, block: TNodeId, value: SymbolId, accessor: SymbolId) {
    let info = l.u.info(accessor);
    let info = l.u.result_type(info);
    for t in super::explicit_outer::terms(l, block, true) {
        if matches!(l.tpd.tree(t), Tree::Ident { .. }) && l.tpd.sym(t) == Some(value) {
            let fun = l.ident(accessor, l.tpd.span(t));
            let args = l.tpd.list(&[]);
            let node = l.tpd.get_mut(t);
            (node.tree, node.sym, node.tpe) = (Tree::Apply { fun, args }, None, Some(info));
        }
    }
}
