//! The local values a local class uses (dotty's `Dependencies` for classes): free in the class
//! where its methods use them, else only in its constructor.

use super::walk::children;
use crate::symbols::{has, is_class, is_module_class, is_static};
use crate::{Lower, Result, unsupported};
use tasty::tags::{FIELDaccessor, LAZY, MUTABLE};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The local values of the enclosing methods a local class uses.
pub struct Free {
    /// All, in the order first used.
    pub values: Vec<SymbolId>,
    /// Those its methods use, which need fields.
    pub in_methods: Vec<SymbolId>,
    /// Whether its methods used one before its constructor code.
    pub methods_first: bool,
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
pub fn free_values(l: &Lower, td: TNodeId, class: SymbolId, outer: SymbolId) -> Result<Free> {
    let static_outer =
        is_module_class(l.u, outer) && is_static(l.u, outer) || l.u.symbol(outer).kind == Kind::Package;
    let mut free = Free { values: Vec::new(), in_methods: Vec::new(), methods_first: false };
    let mut todo = vec![(td, false)];
    while let Some((t, in_method)) = todo.pop() {
        let inner = in_method || matches!(l.tpd.tree(t), Tree::DefDef { .. }) && is_method(l, l.tpd.sym(t));
        todo.extend(children(l.tpd, t).into_iter().rev().map(|c| (c, inner)));
        let Some(s) = l.tpd.sym(t) else { continue };
        let span = l.tpd.span(t);
        match l.tpd.tree(t) {
            Tree::This { .. } if s != class && !within(l, s, class) && !static_outer => {
                return unsupported(span, "local classes using the enclosing instance");
            }
            Tree::Ident { .. } => {
                let owner = l.u.symbol(s).owner;
                if owner == Some(outer) && !static_outer && !is_class(l.u, s) {
                    return unsupported(span, "local classes using the enclosing instance");
                }
                let local = owner.is_some_and(|o| is_term(l, o) && !within(l, o, class));
                if !local || !matches!(l.u.symbol(s).kind, Kind::Val | Kind::Param) {
                    continue;
                }
                if has(l.u, s, MUTABLE) {
                    return unsupported(span, "local classes capturing variables");
                }
                if free.values.is_empty() {
                    free.methods_first = in_method;
                }
                if !free.values.contains(&s) {
                    free.values.push(s);
                }
                if in_method && !free.in_methods.contains(&s) {
                    free.in_methods.push(s);
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
    s.is_some_and(|s| {
        !crate::symbols::is_constructor(l.u, s) && (!has(l.u, s, FIELDaccessor) || has(l.u, s, LAZY))
    })
}
