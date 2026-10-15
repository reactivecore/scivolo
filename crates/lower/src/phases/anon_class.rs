//! Anonymous classes the phases make, `{ final class $anon extends parents { body }; new $anon() }`
//! (dotty's `tpd.AnonClass`, `ClassDef`).

use crate::Lower;
use crate::definitions::{symbol, type_ref};
use syntax::ast::{Span, TName};
use tasty::tags::{FINAL, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The symbol of a final synthetic class `$anon` of `owner` extending `parents`.
pub fn class_symbol(l: &mut Lower, owner: SymbolId, parents: Vec<TypeId>) -> SymbolId {
    let cls = l.u.define(symbol("$anon", Kind::Class, owner, Flags::default().with(FINAL).with(SYNTHETIC)));
    l.u.set_parents(cls, parents);
    cls
}

/// The block defining class `cls` of members `body` and creating an instance of it: its first
/// parent a class whose constructor `parent_ctor` takes no arguments.
pub fn instance(l: &mut Lower, cls: SymbolId, parent_ctor: SymbolId, body: &[TNodeId]) -> TNodeId {
    let ctor = l.u.define(symbol("<init>", Kind::Def, cls, Flags::default().with(SYNTHETIC)));
    let class_ref = type_ref(l.u, cls);
    let info = nullary(l, class_ref);
    l.u.set_info(ctor, info);
    let unit = type_ref(l.u, l.defs.unit);
    let tpt = l.type_tree(unit, Span::NONE);
    let constr = l.build().def_def(ctor, &[&[]], tpt, None, Span::NONE);
    l.positioned(constr);
    let parents = l.u.parents(cls);
    let mut parent_trees = vec![super_call(l, parents[0], parent_ctor)];
    for &p in &parents[1..] {
        parent_trees.push(l.type_tree(p, Span::NONE));
    }
    let parents = l.tpd.list(&parent_trees);
    let body = l.tpd.list(body);
    let template = Tree::Template { constr, parents, self_: None, body };
    let template = l.tpd.node(template, Span::NONE, None, Some(cls));
    l.positioned(template);
    let name = TName { name: l.names.intern("$anon"), is_type: true };
    let td = l.tpd.node(Tree::TypeDef { name, rhs: template }, Span::NONE, None, Some(cls));
    l.positioned(td);
    let new = new_call(l, class_ref, ctor);
    let block = l.block(&[td], new, Span::NONE);
    l.positioned(block)
}

/// `new parent()` through constructor `ctor`, its type arguments those of `parent`.
fn super_call(l: &mut Lower, parent: TypeId, ctor: SymbolId) -> TNodeId {
    let fun = new_select(l, parent, ctor);
    let fun = match l.u.types().get(parent).clone() {
        Type::Applied(_, args) => {
            let args: Vec<TNodeId> = args.into_iter().map(|a| l.type_tree(a, Span::NONE)).collect();
            let method = nullary(l, parent);
            let applied = l.build().type_apply(fun, &args, method, Span::NONE);
            l.positioned(applied)
        }
        _ => fun,
    };
    let call = l.apply(fun, &[], parent, Span::NONE);
    l.positioned(call)
}

/// The type of a method of one empty parameter list and result `result`.
fn nullary(l: &mut Lower, result: TypeId) -> TypeId {
    let binder = l.u.fresh_binder();
    l.u.intern_type(Type::Method { binder, params: Vec::new(), result, implicit: false, contextual: false })
}

/// `new tpe()` through constructor `ctor`.
fn new_call(l: &mut Lower, tpe: TypeId, ctor: SymbolId) -> TNodeId {
    let fun = new_select(l, tpe, ctor);
    let call = l.apply(fun, &[], tpe, Span::NONE);
    l.positioned(call)
}

fn new_select(l: &mut Lower, tpe: TypeId, ctor: SymbolId) -> TNodeId {
    let tpt = l.type_tree(tpe, Span::NONE);
    let new = l.tpd.node(Tree::New { tpt }, Span::NONE, Some(tpe), None);
    l.positioned(new);
    let select = l.select(new, ctor, Span::NONE);
    l.positioned(select)
}
