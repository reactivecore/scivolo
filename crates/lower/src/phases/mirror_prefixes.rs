// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! The prefixes of the classes mirrors refer to, as seen from the mirrored type (dotty's
//! `childPrefix`).

use super::local_free::within;
use crate::Lower;
use crate::definitions::type_ref;
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// `new C` of case class `c`, through its prefix in mirrored type `mono` if it has one.
pub fn new_seen_from(l: &mut Lower, c: SymbolId, mono: Option<TypeId>) -> TNodeId {
    let Some(pre) = mono.and_then(|m| class_prefix(l, m, c)) else { return l.new_(c, Span::NONE) };
    let tpe = l.u.intern_type(Type::Sym(Some(pre), c));
    let tpt = l.type_tree(tpe, Span::NONE);
    l.tpd.node(Tree::New { tpt }, Span::NONE, Some(tpe), None)
}

/// The prefix of class `class` in type `param` if it is a member of a class.
pub fn class_prefix(l: &mut Lower, param: TypeId, class: SymbolId) -> Option<TypeId> {
    let owner = l.u.symbol(class).owner?;
    if l.u.symbol(owner).kind != Kind::Class {
        return None;
    }
    let mut t = l.u.dealias(param);
    if let Type::Applied(tycon, _) = *l.u.types().get(t) {
        t = tycon;
    }
    match *l.u.types().get(t) {
        Type::Sym(Some(pre), s) if s == class => Some(pre),
        _ => None,
    }
}

/// The prefix of child `c` of sealed class `sealed` of prefix `pre`, seen from class `from`:
/// `pre` up to the common owner, then the objects down to `c`, the `this` of those enclosing
/// `from` (dotty's `childPrefix`); `None` where it is no such path.
pub fn child_prefix(
    l: &mut Lower,
    pre: TypeId,
    (sealed, c): (SymbolId, SymbolId),
    from: SymbolId,
) -> Option<TypeId> {
    let owners = |l: &Lower, s: SymbolId| -> Vec<SymbolId> {
        let mut v: Vec<SymbolId> =
            std::iter::successors(l.u.symbol(s).owner, |&o| l.u.symbol(o).owner).collect();
        v.reverse();
        v
    };
    let (parents, children) = (owners(l, sealed), owners(l, c));
    let common = parents.iter().zip(&children).take_while(|(a, b)| a == b).count();
    let mut prefix = Some(pre);
    for _ in common..parents.len() {
        let Type::Sym(q, _) = *l.u.types().get(prefix?) else { return None };
        prefix = q;
    }
    for &o in &children[common..] {
        let m = l.u.source_module(o).filter(|_| l.u.is_module_class(o))?;
        prefix = Some(match within(l, from, o) {
            true => {
                let this = type_ref(l.u, o);
                l.u.intern_type(Type::This(this))
            }
            false => l.u.intern_type(Type::Sym(prefix, m)),
        });
    }
    prefix
}
