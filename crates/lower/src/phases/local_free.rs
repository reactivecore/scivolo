// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Dependencies.scala
//! The local values a local class uses (dotty's `Dependencies` for classes): free in the class
//! where its methods use them, else only in its constructor.

use super::walk::children;
use crate::symbols::{has, is_static, is_static_module};
use crate::{Lower, Result, unsupported};
use tasty::tags::{FIELDaccessor, LAZY, MUTABLE};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The local values of the enclosing methods a local class uses.
pub struct Free {
    /// Those its constructor takes, its own and those of what it calls.
    pub values: Vec<SymbolId>,
    /// Those its methods use, its own and those of what they call, which need fields.
    pub in_methods: Vec<SymbolId>,
    /// The local methods it calls and the local classes it creates, each with whether from a
    /// method.
    pub called: Vec<(SymbolId, bool)>,
}

/// A local class to lift: its definition, its class, the class whose name prefixes its lifted
/// name, the values it uses.
pub struct Local {
    pub td: TNodeId,
    pub class: SymbolId,
    pub named_in: SymbolId,
    pub free: Free,
}

/// The class a local class in a block of class `outer` is lifted into (dotty's `logicOwner` of
/// a class using no enclosing instance): `outer` if static, else its top-level class.
pub fn lifted_into(l: &Lower, outer: SymbolId) -> SymbolId {
    if is_static(l.u, outer) { outer } else { top_level(l, outer) }
}

/// The outermost class enclosing `s` before local classes were lifted, or `s` (dotty's
/// `topLevelClass`).
pub fn top_level(l: &Lower, s: SymbolId) -> SymbolId {
    let mut c = s;
    while let Some(o) = original_owner(l, c).filter(|&o| l.u.symbol(o).kind != Kind::Package) {
        c = o;
    }
    c
}

/// The owner of `s`, of a lifted local class the one before lifting.
pub fn original_owner(l: &Lower, s: SymbolId) -> Option<SymbolId> {
    l.local_classes.get(&s).copied().or(l.u.symbol(s).owner)
}

pub fn is_term(l: &Lower, s: SymbolId) -> bool {
    matches!(l.u.symbol(s).kind, Kind::Def | Kind::Val | Kind::Param)
}

/// The class `s` is in.
pub fn enclosing_class(l: &Lower, mut s: SymbolId) -> SymbolId {
    while is_term(l, s) {
        s = l.u.symbol(s).owner.expect("a term's owner");
    }
    s
}

/// Whether class `c` is defined in a term.
pub fn is_local(l: &Lower, c: SymbolId) -> bool {
    l.u.symbol(c).owner.is_some_and(|o| is_term(l, o))
}

/// Whether `s` is `c` or defined in it.
pub fn within(l: &Lower, mut s: SymbolId, c: SymbolId) -> bool {
    loop {
        if s == c {
            return true;
        }
        let Some(o) = l.u.symbol(s).owner else { return false };
        s = o;
    }
}

/// The local values of the enclosing methods class definition `td` of `class` refers to; fails
/// where it needs an instance of the enclosing class `outer`.
pub fn free_values(l: &mut Lower, td: TNodeId, class: SymbolId, outer: SymbolId) -> Result<Free> {
    let mut created = Vec::new();
    for t in super::walk::subtrees(l.tpd, td) {
        if let Tree::New { tpt } = *l.tpd.tree(t) {
            let tpe = l.tpd.tpe(tpt);
            created.extend(l.u.class_symbol(tpe).map(|c| (t, c)));
        }
    }
    let l: &Lower = l;
    let static_outer = is_static_module(l.u, outer) || l.u.symbol(outer).kind == Kind::Package;
    let mut free = Free { values: Vec::new(), in_methods: Vec::new(), called: Vec::new() };
    let mut todo = vec![(td, false)];
    while let Some((t, in_method)) = todo.pop() {
        let inner = in_method || matches!(l.tpd.tree(t), Tree::DefDef { .. }) && is_method(l, l.tpd.sym(t));
        if !super::walk::is_type_tree(l, t) {
            todo.extend(children(l.tpd, t).into_iter().rev().map(|c| (c, inner)));
        }
        let created = created.iter().find(|&&(n, _)| n == t).map(|&(_, c)| c);
        let other = created.filter(|&c| is_local(l, c) && !within(l, c, class));
        if let Some(c) = other.filter(|&c| !free.called.contains(&(c, in_method))) {
            free.called.push((c, in_method));
        }
        let Some(s) = l.tpd.sym(t) else { continue };
        let span = l.tpd.span(t);
        match l.tpd.tree(t) {
            Tree::This { .. } if s != class && !within(l, s, class) && !static_outer => {
                return unsupported(span, "local classes using the enclosing instance");
            }
            Tree::Ident { .. } => {
                let owner = l.u.symbol(s).owner;
                if owner == Some(outer) && !static_outer && !l.u.symbol(s).kind.is_type() {
                    return unsupported(span, "local classes using the enclosing instance");
                }
                let outer_param =
                    owner.is_some_and(|o| l.u.is_constructor(o) && l.u.symbol(o).owner == Some(outer));
                if outer_param && !static_outer {
                    return unsupported(span, "local classes using the enclosing instance");
                }
                let local = owner.is_some_and(|o| is_term(l, o) && !within(l, o, class));
                if local && l.u.symbol(s).kind == Kind::Def {
                    if !free.called.contains(&(s, in_method)) {
                        free.called.push((s, in_method));
                    }
                    continue;
                }
                if !local || !matches!(l.u.symbol(s).kind, Kind::Val | Kind::Param) {
                    continue;
                }
                if has(l.u, s, MUTABLE) {
                    return unsupported(span, "local classes capturing variables");
                }
                let refs = if in_method { &mut free.in_methods } else { &mut free.values };
                if !refs.contains(&s) {
                    refs.push(s);
                }
            }
            _ => {}
        }
    }
    Ok(free)
}

/// Whether `s` is a method whose body runs outside the constructor: not a constructor, nor a
/// field's getter, whose body is its initializer.
pub fn is_method(l: &Lower, s: Option<SymbolId>) -> bool {
    s.is_some_and(|s| !l.u.is_constructor(s) && (!has(l.u, s, FIELDaccessor) || has(l.u, s, LAZY)))
}
