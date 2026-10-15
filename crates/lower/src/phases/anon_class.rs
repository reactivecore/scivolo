//! Anonymous classes the phases make, `{ final class $anon extends parents { body }; new $anon() }`
//! (dotty's `tpd.AnonClass`, `ClassDef`).

use crate::Lower;
use crate::definitions::{symbol, type_ref};
use syntax::ast::{Span, TName};
use tasty::tags::{FINAL, OVERRIDE, SYNTHETIC};
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

/// The member of class `cls` overriding `m` as a member of `site`, and its type parameters,
/// which its type refers to as `m`'s type refers to `m`'s.
pub fn override_sym(l: &mut Lower, cls: SymbolId, m: SymbolId, site: TypeId) -> (SymbolId, Vec<SymbolId>) {
    let name = l.u.symbol(m).name.clone();
    let flags = Flags::default().with(SYNTHETIC).with(FINAL).with(OVERRIDE);
    let s = l.u.define(symbol(&name, Kind::Def, cls, flags));
    let mut info = l.u.member_info(site, m);
    let mut tparams = Vec::new();
    if let Type::Poly { binder, params, .. } = l.u.types().get(info).clone() {
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        tparams = names.iter().map(|n| l.u.define(symbol(n, Kind::TypeParam, s, Flags::default()))).collect();
        let args: Vec<TypeId> = tparams.iter().map(|&t| l.u.intern_type(Type::Sym(None, t))).collect();
        info = l.u.subst(info, binder, &names, &args);
        info = l.u.map_type(info, &mut |u, t| match *u.types().get(t) {
            Type::Sym(_, p) if u.symbol(p).owner == Some(m) => {
                names.iter().position(|n| *n == u.symbol(p).name).map(|i| args[i])
            }
            _ => None,
        });
        let Type::Poly { params, .. } = l.u.types().get(info).clone() else {
            unreachable!("a polymorphic type")
        };
        for (&t, (_, bounds)) in tparams.iter().zip(params) {
            l.u.set_info(t, bounds);
        }
    }
    l.u.set_info(s, info);
    (s, tparams)
}

/// Parameter `name` of type `info` of `method`.
pub fn param(l: &mut Lower, method: SymbolId, name: &str, info: TypeId) -> SymbolId {
    let p = l.u.define(symbol(name, Kind::Param, method, Flags::default()));
    l.u.set_info(p, info);
    p
}

/// The definition of `method` of type parameters `tparams` and parameters `params`, these at
/// `at`, of result type `result`.
pub fn def(
    l: &mut Lower,
    method: SymbolId,
    (tparams, params): (&[SymbolId], &[SymbolId]),
    (result, rhs): (TypeId, TNodeId),
    at: Span,
) -> TNodeId {
    let mut tdefs = Vec::new();
    for &t in tparams {
        let name = l.u.symbol(t).name.clone();
        let name = TName { name: l.names.intern(&name), is_type: true };
        let info = l.u.info(t);
        let tpt = l.type_tree(info, at);
        tdefs.push(l.tpd.node(Tree::TypeDef { name, rhs: tpt }, at, None, Some(t)));
    }
    let vdefs: Vec<TNodeId> = params.iter().map(|&p| l.param_def(p, at)).collect();
    let tpt = l.type_tree(result, Span::NONE);
    let clauses: Vec<&[TNodeId]> = if tdefs.is_empty() { vec![&vdefs] } else { vec![&tdefs, &vdefs] };
    let d = l.build().def_def(method, &clauses, tpt, Some(rhs), Span::NONE);
    l.positioned(d)
}

/// The pattern `_` of type `tpe`.
pub fn wildcard(l: &mut Lower, tpe: TypeId) -> TNodeId {
    let name = l.term_name("_");
    let class = match *l.u.types().get(tpe) {
        Type::Sym(_, s) => Some(s),
        _ => None,
    };
    l.tpd.node(Tree::Ident { name }, Span::NONE, Some(tpe), class)
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
