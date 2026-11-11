// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! Anonymous classes the phases make, `{ final class $anon extends parents { body }; new $anon() }`
//! (dotty's `tpd.AnonClass`, `ClassDef`).

use crate::Lower;
use crate::definitions::{symbol, type_ref};
use syntax::ast::{Span, TName};
use tasty::tags::{FINAL, OVERRIDE, SYNTHETIC};
use tasty::{Flags, Kind, SymbolId, Type, TypeId, nme};
use typer::tpd::{TNodeId, Tree};

/// The symbol of a final synthetic class `$anon` of `owner` extending `parents`.
pub fn class_symbol(l: &mut Lower, owner: SymbolId, parents: Vec<TypeId>) -> SymbolId {
    let cls =
        l.u.define(symbol(nme::ANON_CLASS, Kind::Class, owner, Flags::default().with(FINAL).with(SYNTHETIC)));
    l.u.set_parents(cls, parents);
    cls
}

/// The block defining class `cls` of members `body`, at `span` if it exists, and creating an
/// instance of it: its first parent a class whose constructor `parent_ctor` takes no arguments or
/// only repeated ones.
pub fn instance(
    l: &mut Lower,
    cls: SymbolId,
    parent_ctor: SymbolId,
    body: &[TNodeId],
    span: Span,
) -> TNodeId {
    let ctor = l.u.define(symbol(nme::CONSTRUCTOR, Kind::Def, cls, Flags::default().with(SYNTHETIC)));
    let class_ref = type_ref(l.u, cls);
    let info = nullary(l, class_ref);
    l.u.set_info(ctor, info);
    let unit = type_ref(l.u, l.u.defn().unit);
    let tpt = l.type_tree(unit, Span::NONE);
    let constr = l.build().def_def(ctor, &[&[]], tpt, None, span);
    l.positioned(constr);
    let parents = l.u.parents(cls);
    let mut parent_trees = vec![super_call(l, parents[0], parent_ctor, span)];
    for &p in &parents[1..] {
        parent_trees.push(l.type_tree(p, Span::NONE));
    }
    let parents = l.tpd.list(&parent_trees);
    let body = l.tpd.list(body);
    let template = Tree::Template { constr, parents, self_: None, body };
    let template = l.tpd.node(template, span, None, Some(cls));
    l.positioned(template);
    let name = TName { name: l.names.intern(nme::ANON_CLASS), is_type: true };
    let td = l.tpd.node(Tree::TypeDef { name, rhs: template }, span, None, Some(cls));
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
    if let Some((binder, names)) = l.u.poly(info).map(|p| (p.binder, p.names())) {
        tparams = names
            .iter()
            .map(|&n| l.u.define(symbol(l.u.name(n), Kind::TypeParam, s, Flags::default())))
            .collect();
        let args: Vec<TypeId> = tparams.iter().map(|&t| l.u.intern_type(Type::Sym(None, t))).collect();
        info = l.u.subst(info, binder, &names, &args);
        info = l.u.map_type(info, &mut |u, t| match *u.ty(t) {
            Type::Sym(_, p) if u.symbol(p).owner == Some(m) => {
                names.iter().position(|&n| u.name(n) == u.symbol(p).name).map(|i| args[i])
            }
            _ => None,
        });
        let Some(bounds) = l.u.poly(info).map(|p| p.param_types()) else {
            unreachable!("a polymorphic type")
        };
        for (&t, bounds) in tparams.iter().zip(bounds) {
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
    let tdefs: Vec<TNodeId> = tparams.iter().map(|&t| l.type_param_def(t, at)).collect();
    let vdefs: Vec<TNodeId> = params.iter().map(|&p| l.param_def(p, at)).collect();
    let tpt = l.type_tree(result, Span::NONE);
    let clauses: Vec<&[TNodeId]> = if tdefs.is_empty() { vec![&vdefs] } else { vec![&tdefs, &vdefs] };
    let d = l.build().def_def(method, &clauses, tpt, Some(rhs), Span::NONE);
    l.positioned(d)
}

/// The pattern `_` of type `tpe`.
pub fn wildcard(l: &mut Lower, tpe: TypeId) -> TNodeId {
    let name = l.term_name("_");
    let class = l.u.sym_of(tpe);
    l.tpd.node(Tree::Ident { name }, Span::NONE, Some(tpe), class)
}

/// `new parent()` through constructor `ctor`, its type arguments those of `parent`, an empty
/// sequence for a repeated parameter, at `span` if it exists.
fn super_call(l: &mut Lower, parent: TypeId, ctor: SymbolId, span: Span) -> TNodeId {
    let fun = new_select(l, parent, ctor, span);
    let fun = match l.u.applied(parent).map(|(_, a)| a.to_vec()) {
        Some(args) => {
            let args: Vec<TNodeId> = args.into_iter().map(|a| l.type_tree(a, span)).collect();
            let method = nullary(l, parent);
            let applied = l.build().type_apply(fun, &args, method, span);
            l.positioned(applied)
        }
        None => fun,
    };
    let args = empty_varargs(l, parent, ctor);
    let call = l.apply(fun, &args, parent, span);
    l.positioned(call)
}

/// No argument for constructor `ctor` of `parent`, or an empty sequence if it takes one repeated
/// parameter.
fn empty_varargs(l: &mut Lower, parent: TypeId, ctor: SymbolId) -> Vec<TNodeId> {
    let mut info = l.u.info(ctor);
    if let Some((binder, names, result)) = l.u.poly(info).map(|p| (p.binder, p.names(), p.result)) {
        let Some(targs) = l.u.applied(parent).map(|(_, a)| a.to_vec()) else { return Vec::new() };
        info = l.u.subst(result, binder, &names, &targs);
    }
    let Some(&[(_, p)]) = l.u.method(info).map(|m| m.params) else { return Vec::new() };
    let Some(elem) = l.u.repeated_elem(p) else { return Vec::new() };
    let repeated = l.u.class_ref(tasty::defn::REPEATED);
    let repeated = l.u.intern_applied(repeated, &[elem]);
    let elemtpt = l.type_tree(elem, Span::NONE);
    let elems = l.tpd.list(&[]);
    let literal = l.tpd.node(Tree::SeqLiteral { elems, elemtpt }, Span::NONE, None, None);
    let tpt = l.type_tree(repeated, Span::NONE);
    vec![l.tpd.node(Tree::Typed { expr: literal, tpt }, Span::NONE, Some(repeated), None)]
}

/// The type of a method of one empty parameter list and result `result`.
fn nullary(l: &mut Lower, result: TypeId) -> TypeId {
    let binder = l.u.fresh_binder();
    l.u.intern_method(binder, &[], result, false, false)
}

/// `new tpe()` through constructor `ctor`.
fn new_call(l: &mut Lower, tpe: TypeId, ctor: SymbolId) -> TNodeId {
    let fun = new_select(l, tpe, ctor, Span::NONE);
    let call = l.apply(fun, &[], tpe, Span::NONE);
    l.positioned(call)
}

fn new_select(l: &mut Lower, tpe: TypeId, ctor: SymbolId, span: Span) -> TNodeId {
    let tpt = l.type_tree(tpe, span);
    let new = l.tpd.node(Tree::New { tpt }, span, Some(tpe), None);
    l.positioned(new);
    let select = l.select(new, ctor, span);
    l.positioned(select)
}
