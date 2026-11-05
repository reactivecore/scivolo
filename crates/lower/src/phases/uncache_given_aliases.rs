// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/UncacheGivenAliases.scala
//! dotty's `UncacheGivenAliases`: a given alias whose right-hand side has type `this`, `this.y`
//! or `y` of a stable `y` becomes a parameterless method.

use super::local_free::enclosing_class;
use super::walk::subtrees;
use crate::Lower;
use crate::symbols::has;
use tasty::tags::{GIVEN, LAZY, MUTABLE, STABLE};
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, root: TNodeId) {
    for t in subtrees(l.tpd, root) {
        let (&Tree::ValDef { name, tpt, rhs: Some(rhs) }, Some(s)) = (l.tpd.tree(t), l.tpd.sym(t)) else {
            continue;
        };
        if !has(l.u, s, LAZY) || !has(l.u, s, GIVEN) || needs_cache(l, s, l.tpd.tpe(rhs)) {
            continue;
        }
        let flags = l.u.symbol(s).flags.without(LAZY);
        l.u.set_flags(s, flags);
        l.u.set_kind(s, Kind::Def);
        let paramss = l.tpd.clauses(&[]);
        l.tpd.get_mut(t).tree = Tree::DefDef { name, paramss, tpt, rhs: Some(rhs) };
        l.uncached_givens.push(s);
    }
}

/// Makes the given aliases `transform` made methods lazy values again, as later units see them.
pub fn restore(l: &mut Lower) {
    for s in std::mem::take(&mut l.uncached_givens) {
        let flags = l.u.symbol(s).flags.with(LAZY);
        l.u.set_flags(s, flags);
        l.u.set_kind(s, Kind::Val);
    }
}

/// Whether given alias `s` must cache a right-hand side of type `rhs`: unless `rhs` is a `this`
/// type, or a stable reference without prefix or through the `this` of the class enclosing `s`.
fn needs_cache(l: &mut Lower, s: SymbolId, rhs: TypeId) -> bool {
    let (prefix, y) = match *l.u.types().get(rhs) {
        Type::This(_) => return false,
        Type::Sym(prefix, y) => (prefix, y),
        Type::TermRef(prefix, _) => match l.u.resolve_ref(rhs) {
            Some(y) => (Some(prefix), y),
            None => return true,
        },
        _ => return true,
    };
    let this_prefix = match prefix.map(|p| l.u.types().get(p).clone()) {
        None | Some(Type::NoPrefix) => true,
        Some(Type::This(c)) => {
            let owner = l.u.symbol(s).owner.expect("a given alias's owner");
            l.u.class_symbol(c) == Some(enclosing_class(l, owner))
        }
        _ => false,
    };
    !this_prefix || !is_stable(l, y)
}

/// Whether reference `y` is stable: an object, a value or parameter that is neither mutable nor
/// by-name, a stable method, or a variable of a singleton type (dotty's `TermRef.isStable`).
fn is_stable(l: &mut Lower, y: SymbolId) -> bool {
    match l.u.symbol(y).kind {
        _ if l.u.is_module(y) => true,
        Kind::Val if has(l.u, y, MUTABLE) => {
            let info = l.u.info(y);
            match *l.u.types().get(info) {
                Type::This(_) | Type::TermRef(..) | Type::Constant(..) | Type::Str(_) => true,
                Type::Sym(_, t) => l.u.symbol(t).kind.is_term(),
                _ => false,
            }
        }
        Kind::Val | Kind::Param => !super::first_transform::is_by_name(l, y),
        Kind::Def => has(l.u, y, STABLE),
        _ => false,
    }
}
