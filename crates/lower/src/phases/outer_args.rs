// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ExplicitOuter.scala
//! The outer arguments of dotty's `ExplicitOuter`: `new p.C(args)` of a class `C` taking its
//! enclosing instance becomes `new p.C(p, args)`, `this` for a prefix `Outer.this`.

use super::explicit_outer::{enclosing_instance, outer_chain, terms};
use super::leading_params::with_leading_params;
use super::local_free::{enclosing_class, is_term, within};
use crate::definitions::{symbol, type_ref};
use crate::symbols::{has, is_static, is_static_module};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::tags::PRIVATE;
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Passes the enclosing instance to each `new C` in class definition `td` of a class `C` that
/// takes it.
pub fn pass_outer(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    for t in terms(l, td, false) {
        let Tree::Apply { fun, args } = *l.tpd.tree(t) else { continue };
        let ctor = match *l.tpd.tree(fun) {
            Tree::TypeApply { fun: f, .. } => f,
            _ => fun,
        };
        let Tree::Select { qualifier: new, .. } = *l.tpd.tree(ctor) else { continue };
        if !matches!(l.tpd.tree(new), Tree::New { .. }) {
            continue;
        }
        let tpe = l.tpd.tpe(new);
        let Some(created) = l.u.class_symbol(tpe) else { continue };
        let span = l.tpd.span(t);
        let library = l.u.symbol(created).tree.is_some();
        match enclosing_instance(l, created) {
            Some(outer) if library => match l.tpd.sym(ctor) {
                Some(c) if !has(l.u, created, PRIVATE) => declare_library(l, created, c, outer),
                _ => return unsupported(span, "private library classes nested in classes"),
            },
            _ if !l.outers.contains_key(&created) => continue,
            _ => {}
        }
        let at = Span::at(l.tpd.span(ctor).start());
        let owner = l.u.symbol(created).owner.filter(|&o| is_term(l, o));
        let instance = match owner.map(|o| enclosing_class(l, o)) {
            Some(outer) if outer == class => l.this(class, at),
            Some(outer) if outer_chain(l, class, outer).is_some() => l.this(outer, Span::NONE),
            Some(_) => return unsupported(span, "creating a local class from another class"),
            None => instance(l, class, tpe, at)?,
        };
        let outer_this = l.tpd.sym(instance).filter(|&c| c != class);
        if let (Tree::This { .. }, Some(c)) = (l.tpd.tree(instance), outer_this) {
            let end = match is_static_module(l.u, c) {
                true => l.tpd.span(fun),
                false => Span::at(l.tpd.span(fun).end()),
            };
            let qual = match *l.tpd.tree(instance) {
                Tree::This { qual } => qual,
                _ => unreachable!("a `this`"),
            };
            (l.tpd.get_mut(instance).span, l.tpd.get_mut(qual).span) = (end, end);
        }
        let all: Vec<TNodeId> = std::iter::once(instance).chain(l.tpd.items(args).iter().copied()).collect();
        let args = l.tpd.list(&all);
        l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
    }
    Ok(())
}

/// Gives constructor `ctor` of library class `class` the leading parameter `$outer` of the type
/// of `outer` unless it has it, and `class` its entry in `l.outers` (dotty's `needsOuterAlways`).
fn declare_library(l: &mut Lower, class: SymbolId, ctor: SymbolId, outer: SymbolId) {
    let tpe = type_ref(l.u, outer);
    let info = l.u.info(ctor);
    let first = match l.u.types().get(info) {
        Type::Poly { result, .. } => *result,
        _ => info,
    };
    let declared = matches!(l.u.types().get(first), Type::Method { params, .. }
        if params.first().is_some_and(|(n, _)| n == "$outer"));
    if declared && l.outers.contains_key(&class) {
        return;
    }
    let param = l.u.define(symbol("$outer", Kind::Param, ctor, Flags::default()));
    l.u.set_info(param, tpe);
    if !declared {
        let info = with_leading_params(l, info, &[param]);
        l.u.set_info(ctor, info);
    }
    l.outers.entry(class).or_insert((None, param));
}

/// The enclosing instance of a class of type `tpe` created in `class`: its prefix as a tree.
fn instance(l: &mut Lower, class: SymbolId, tpe: TypeId, span: Span) -> Result<TNodeId> {
    let prefix = match l.u.types().get(tpe).clone() {
        Type::Applied(tycon, _) => return instance(l, class, tycon, span),
        Type::Sym(Some(p), _) | Type::TypeRef(p, _) => p,
        _ => return unsupported(span, "creating an inner class without a prefix"),
    };
    path(l, class, prefix, span)
}

/// Stable path `p` as a tree in `class` (dotty's `fixThis`): `this` of `class`, also for the
/// object `class` is the class of (dotty's `ref` of a module seen from inside it), of a static
/// object or of a class whose instance `class` reaches through enclosing instances, an object
/// seen from outside it through its value, a local value or parameter, or a value or object
/// selected on such a path.
fn path(l: &mut Lower, class: SymbolId, p: TypeId, span: Span) -> Result<TNodeId> {
    let refused = "creating an inner class through this prefix";
    match l.u.types().get(p).clone() {
        Type::This(c) => {
            let Some(c) = l.u.class_symbol(c) else { return unsupported(span, refused) };
            let module = l.u.is_module_class(c).then(|| l.u.source_module(c)).flatten();
            match module.filter(|_| !within(l, class, c)) {
                _ if c == class || is_static_module(l.u, c) => Ok(l.this(c, Span::NONE)),
                Some(m) => {
                    let owner = l.u.symbol(c).owner.expect("an object's owner");
                    let owner = type_ref(l.u, owner);
                    let this = l.u.intern_type(Type::This(owner));
                    let qualifier = path(l, class, this, span)?;
                    Ok(l.select(qualifier, m, span))
                }
                None if outer_chain(l, class, c).is_some() => Ok(l.this(c, Span::NONE)),
                None => unsupported(span, refused),
            }
        }
        Type::Sym(q, s) if matches!(l.u.symbol(s).kind, Kind::Val | Kind::Param) => {
            let module = l.u.module_class(s).filter(|_| l.u.is_module(s));
            if module == Some(class) {
                return Ok(l.this(class, Span::NONE));
            }
            if module.is_some() && is_static(l.u, s) {
                return Ok(l.ident(s, span));
            }
            if module.is_some_and(|m| within(l, class, m)) {
                return unsupported(span, refused);
            }
            let owner = l.u.symbol(s).owner;
            if owner.is_some_and(|o| is_term(l, o)) {
                return Ok(l.ident(s, span));
            }
            let this_class = match q.map(|q| l.u.types().get(q).clone()) {
                Some(Type::This(c)) => l.u.class_symbol(c),
                _ => None,
            };
            let on_this = q.is_none() || this_class == Some(class);
            let qualifier = match q {
                _ if on_this && owner == Some(class) => l.this(class, span),
                Some(q) => path(l, class, q, span)?,
                None => return unsupported(span, refused),
            };
            Ok(l.select(qualifier, s, span))
        }
        _ => unsupported(span, refused),
    }
}
