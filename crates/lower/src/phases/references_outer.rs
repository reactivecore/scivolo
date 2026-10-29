// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ExplicitOuter.scala
//! dotty's `ExplicitOuter.referencesOuter`: whether a class definition refers to the instance of
//! a class enclosing it that is not a static owner.

use super::explicit_outer::terms;
use super::local_free::{enclosing_class, is_term, within};
use super::walk::is_type_tree;
use crate::Lower;
use crate::symbols::{has, is_static, is_static_owner};
use tasty::tags::{INLINE, LAZY};
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Whether definition `td` of class `class`, the classes in it included, refers to the instance
/// of a class enclosing `class`, also through the type trees of inline methods.
pub fn references_outer(l: &mut Lower, class: SymbolId, td: TNodeId) -> bool {
    let all = terms(l, td, true);
    let inline = |l: &Lower, t| {
        matches!(l.tpd.tree(t), Tree::DefDef { .. }) && l.tpd.sym(t).is_some_and(|s| has(l.u, s, INLINE))
    };
    let inline_defs: Vec<TNodeId> = all.iter().copied().filter(|&t| inline(l, t)).collect();
    all.into_iter().any(|t| contains_outer_refs(l, class, t))
        || inline_defs.into_iter().any(|d| {
            let types: Vec<TNodeId> = terms(l, d, true).into_iter().filter(|&t| is_type_tree(l, t)).collect();
            types
                .into_iter()
                .any(|t| l.tpd.get(t).tpe.is_some_and(|tpe| contains_outer_refs_anywhere(l, class, tpe)))
        })
}

/// Whether a singleton part of `tpe` or of its aliases refers to an enclosing instance, as the
/// expansions of inline methods do (dotty's `containsOuterRefsAnywhere`).
fn contains_outer_refs_anywhere(l: &mut Lower, class: SymbolId, tpe: TypeId) -> bool {
    let (mut singletons, mut aliases) = (Vec::new(), Vec::new());
    l.u.map_type(tpe, &mut |u, part| {
        match *u.types().get(part) {
            Type::This(_) | Type::TermRef(..) => singletons.push(part),
            Type::Sym(_, s) if u.symbol(s).kind.is_term() => singletons.push(part),
            Type::Sym(..) | Type::TypeRef(..) => {
                let alias = u.dealias(part);
                if alias != part {
                    aliases.push(alias);
                }
            }
            _ => {}
        }
        None
    });
    singletons.into_iter().any(|p| is_outer_ref(l, class, p))
        || aliases.into_iter().any(|a| contains_outer_refs_anywhere(l, class, a))
}

fn contains_outer_refs(l: &mut Lower, class: SymbolId, t: TNodeId) -> bool {
    match *l.tpd.tree(t) {
        Tree::This { .. } | Tree::Ident { .. } => {
            l.tpd.get(t).tpe.is_some_and(|tpe| is_outer_ref(l, class, tpe))
        }
        Tree::New { .. } => {
            let Some(tpe) = l.tpd.get(t).tpe else { return false };
            let tpe = l.u.dealias(tpe);
            let Some(created) = l.u.class_symbol(tpe) else { return false };
            let owner = l.u.symbol(created).owner.expect("a class's owner");
            is_outer_sym(l, class, enclosing_class(l, owner))
                || has_outer_prefix(l, class, tpe)
                || is_term(l, owner) && class != created && within(l, class, created)
        }
        Tree::Select { qualifier, name } => {
            let Some(hops) = typer::inlining::outer_this::outer_hops(l.names.text(name.name)) else {
                return false;
            };
            let Some(mut target) = l.u.class_symbol(l.tpd.tpe(qualifier)) else { return false };
            for _ in 0..hops {
                let owner = l.u.symbol(target).owner.expect("a class's owner");
                target = enclosing_class(l, owner);
            }
            is_outer_sym(l, class, target)
        }
        Tree::TypeApply { fun, args } if is_type_test(l, fun) => {
            let Some(&arg) = l.tpd.items(args).first() else { return false };
            let tpe = l.tpd.tpe(arg);
            let tpe = l.u.dealias(tpe);
            contains_outer_refs_at_top_level(l, class, tpe)
        }
        _ => false,
    }
}

/// Whether `fun` is `isInstanceOf` or `$isInstanceOf$`.
fn is_type_test(l: &Lower, fun: TNodeId) -> bool {
    l.tpd.sym(fun).is_some_and(|s| s == l.defs.is_instance_of || s == l.defs.type_test)
}

/// Whether `s` is a class properly enclosing `class` that is not a static owner.
fn is_outer_sym(l: &Lower, class: SymbolId, s: SymbolId) -> bool {
    !is_static_owner(l.u, s) && class != s && within(l, class, s)
}

fn is_outer_ref(l: &mut Lower, class: SymbolId, tpe: TypeId) -> bool {
    match *l.u.types().get(tpe) {
        Type::This(c) => l.u.class_symbol(c).is_some_and(|c| is_outer_sym(l, class, c)),
        Type::Sym(_, s) if !l.u.symbol(s).kind.is_term() => false,
        Type::Sym(Some(pre), s) if !matches!(l.u.types().get(pre), Type::NoPrefix) => {
            !is_static(l.u, s) && is_outer_ref(l, class, pre)
        }
        Type::Sym(_, s) => {
            let Some(owner) = l.u.symbol(s).owner else { return false };
            let hoistable = l.u.symbol(s).kind == Kind::Def || has(l.u, s, LAZY) || l.u.is_module(s);
            let enclosing = l.u.symbol(class).owner.map(|o| enclosing_class(l, o));
            hoistable && is_outer_sym(l, class, enclosing_class(l, owner))
                || enclosing.is_some_and(|e| within(l, e, owner))
        }
        Type::TermRef(pre, _) => is_outer_ref(l, class, pre),
        _ => false,
    }
}

fn has_outer_prefix(l: &mut Lower, class: SymbolId, tpe: TypeId) -> bool {
    match *l.u.types().get(tpe) {
        Type::Applied(tycon, _) => has_outer_prefix(l, class, tycon),
        Type::Sym(Some(pre), _) | Type::TypeRef(pre, _) => is_outer_ref(l, class, pre),
        _ => false,
    }
}

fn contains_outer_refs_at_top_level(l: &mut Lower, class: SymbolId, tpe: TypeId) -> bool {
    match *l.u.types().get(tpe) {
        Type::And(a, b) | Type::Or(a, b, _) => {
            contains_outer_refs_at_top_level(l, class, a) || contains_outer_refs_at_top_level(l, class, b)
        }
        Type::This(_) | Type::TermRef(..) => is_outer_ref(l, class, tpe),
        Type::Sym(_, s) if l.u.symbol(s).kind.is_term() => is_outer_ref(l, class, tpe),
        _ => false,
    }
}
