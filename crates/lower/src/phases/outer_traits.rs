//! dotty's `ExplicitOuter` for traits: a trait in a class gets a deferred accessor
//! `O$T$$$outer()` that its references to the enclosing instance call; a class gets, for each
//! trait it mixes in that may use its enclosing instance, an implementation of that trait's
//! accessor returning the prefix of the trait in its type.

use super::explicit_outer::{enclosing_instance, outer_accessor, outer_accessor_name, outer_this, terms};
use crate::definitions::{symbol, type_ref};
use crate::symbols::{is_static, is_static_owner};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::FINAL;
use tasty::{Flags, Kind, SymbolId, Type, TypeId, Universe};
use typer::tpd::{TNodeId, Tree};

/// The class whose instance class or trait `c` can use (dotty's `needsOuterIfReferenced`).
pub fn outer_class(l: &Lower, c: SymbolId) -> Option<SymbolId> {
    if !l.u.is_trait(c) {
        return enclosing_instance(l, c);
    }
    let owner = l.u.symbol(c).owner?;
    let nested = l.u.is_class(owner) && !is_static_owner(l.u, owner) && !is_static(l.u, c);
    (nested && !is_pure_interface(l.u, c)).then_some(owner)
}

/// Whether trait `t` has only abstract members and types (dotty's `PureInterface`).
fn is_pure_interface(u: &Universe, t: SymbolId) -> bool {
    let concrete = |s: SymbolId| match u.symbol(s).tree.is_some() {
        true => u.body(s).is_some(),
        false => !u.is_deferred(s),
    };
    u.no_inits(t)
        && u.symbol(t).decls.iter().all(|&d| match u.symbol(d).kind {
            Kind::Class => false,
            k if k.is_term() => u.is_constructor(d) || !concrete(d),
            _ => true,
        })
}

/// Adds to class definition `td` of `class`, not a trait, the implementations of the outer
/// accessors of the traits it mixes in that may need one; whether there are any.
pub fn implement(l: &mut Lower, td: TNodeId, class: SymbolId) -> Result<bool> {
    if l.u.is_trait(class) {
        return Ok(false);
    }
    let linearization = l.u.linearization(class);
    let mixins: Vec<SymbolId> = linearization[1..].iter().copied().take_while(|&b| l.u.is_trait(b)).collect();
    let mut defs = Vec::new();
    for m in mixins {
        let _ = l.u.decls(m);
        let Some(outer) = outer_class(l, m) else { continue };
        let span = end_span(l, td);
        let prefix = prefix(l, class, m, outer);
        let rhs = path(l, class, prefix, span)?;
        let result = result_type(l, class, outer, prefix);
        let name = outer_accessor_name(l, m);
        let sym = l.u.define(symbol(&name, Kind::Def, class, Flags::default().with(FINAL)));
        let info = method_type(l, result);
        l.u.set_info(sym, info);
        let tpt = l.type_tree(result, span);
        defs.push(l.method_def(sym, &[], tpt, rhs, span));
    }
    let any = !defs.is_empty();
    append(l, td, &defs);
    Ok(any)
}

/// Gives trait definition `td` of `trait_` its deferred outer accessor and makes its references
/// to enclosing instances call it.
pub fn add_outer(l: &mut Lower, td: TNodeId, trait_: SymbolId) {
    let accessor = outer_accessor(l, trait_);
    l.u.set_deferred(accessor, true);
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a trait definition") };
    let Tree::Template { body, parents, .. } = *l.tpd.tree(template) else { unreachable!("a template") };
    let stats: Vec<TNodeId> = l.tpd.items(body).iter().chain(l.tpd.items(parents)).copied().collect();
    let stats: Vec<TNodeId> =
        stats.into_iter().filter(|&s| !matches!(l.tpd.tree(s), Tree::TypeDef { .. })).collect();
    for stat in stats {
        for t in terms(l, stat, false) {
            let Some(chain) = outer_this(l, t, trait_) else { continue };
            let span = l.tpd.span(t);
            let mut read = l.this(trait_, span);
            let mut from = trait_;
            for c in chain {
                let accessor = outer_accessor(l, from);
                let fun = l.select(read, accessor, span);
                let tpe = type_ref(l.u, c);
                read = l.apply(fun, &[], tpe, span);
                from = c;
            }
            l.copy_node(t, read);
        }
    }
    let span = end_span(l, td);
    let info = l.u.info(accessor);
    let result = l.u.result_type(info);
    let tpt = l.type_tree(result, span);
    let def = l.build().def_def(accessor, &[&[]], tpt, None, span);
    append(l, td, &[def]);
}

/// The point after the body of class definition `td`, where dotty's copied template places the
/// members it adds.
fn end_span(l: &Lower, td: TNodeId) -> Span {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    Span::at(l.tpd.span(template).end().max(l.tpd.span(td).point()))
}

fn append(l: &mut Lower, td: TNodeId, defs: &[TNodeId]) {
    let Tree::TypeDef { rhs: template, .. } = *l.tpd.tree(td) else { unreachable!("a class definition") };
    let Tree::Template { constr, parents, self_, body } = *l.tpd.tree(template) else { return };
    let stats: Vec<TNodeId> = l.tpd.items(body).iter().chain(defs).copied().collect();
    let body = l.tpd.list(&stats);
    l.tpd.get_mut(template).tree = Tree::Template { constr, parents, self_, body };
}

/// The prefix of trait `m` in the type of `this` of `class`, `this` of the class enclosing `m`
/// for none (dotty's `outerPrefix`).
fn prefix(l: &mut Lower, class: SymbolId, m: SymbolId, outer: SymbolId) -> TypeId {
    let this = type_ref(l.u, class);
    let this = l.u.intern_type(Type::This(this));
    let base = l.u.base_type(this, m).map(|b| match *l.u.types().get(b) {
        Type::Applied(tycon, _) => tycon,
        _ => b,
    });
    match base.map(|b| l.u.types().get(b).clone()) {
        Some(Type::Sym(Some(pre), _) | Type::TypeRef(pre, _)) => pre,
        _ => {
            let outer = type_ref(l.u, outer);
            l.u.intern_type(Type::This(outer))
        }
    }
}

/// The result type of the implementation in `class` of an accessor of an instance of `outer`:
/// `outer`'s type where the class enclosing `class` derives from it, else the prefix's.
fn result_type(l: &mut Lower, class: SymbolId, outer: SymbolId, prefix: TypeId) -> TypeId {
    let mut enclosing = l.u.symbol(class).owner;
    while let Some(e) = enclosing.filter(|&e| l.u.symbol(e).kind.is_term()) {
        enclosing = l.u.symbol(e).owner;
    }
    match enclosing.filter(|&e| l.u.is_class(e)) {
        Some(e) if l.u.derives_from(e, outer) => type_ref(l.u, outer),
        _ => l.u.widen(prefix),
    }
}

fn method_type(l: &mut Lower, result: TypeId) -> TypeId {
    let binder = l.u.fresh_binder();
    l.u.intern_type(Type::Method { binder, params: Vec::new(), result, implicit: false, contextual: false })
}

/// Stable prefix `p` as a tree in `class`: `this` of a class, an object seen from outside it
/// through its value (dotty's `fixThis`), values selected on such paths.
fn path(l: &mut Lower, class: SymbolId, p: TypeId, span: Span) -> Result<TNodeId> {
    match l.u.types().get(p).clone() {
        Type::This(c) => {
            let c = l.u.class_symbol(c).expect("a class's this");
            let module = l.u.is_module_class(c).then(|| l.u.source_module(c)).flatten();
            match module.filter(|_| !super::local_free::within(l, class, c)) {
                Some(m) if is_static(l.u, m) => Ok(l.ident(m, span)),
                Some(_) => unsupported(span, "outer prefixes of mixed-in traits other than paths"),
                None => Ok(l.this(c, span)),
            }
        }
        Type::Sym(q, s) if l.u.symbol(s).kind.is_term() => {
            let static_module = l.u.is_module(s) && is_static(l.u, s);
            let qualifier = match q.map(|q| l.u.types().get(q).clone()) {
                _ if static_module => return Ok(l.ident(s, span)),
                None | Some(Type::Package(_)) => return Ok(l.ident(s, span)),
                Some(_) => path(l, class, q.expect("a prefix"), span)?,
            };
            Ok(l.select(qualifier, s, span))
        }
        _ => unsupported(span, "outer prefixes of mixed-in traits other than paths"),
    }
}
