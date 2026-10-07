//! The local values a local class uses (dotty's `Dependencies` for classes): free in the class
//! where its methods use them, else only in its constructor.

use super::walk::children;
use crate::symbols::{has, is_class, is_module_class, is_static, is_trait};
use crate::{Lower, Result, unsupported};
use tasty::tags::{CASE, FIELDaccessor, LAZY, MUTABLE};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The local values of the enclosing methods a local class uses.
pub struct Free {
    /// All, in the order defined.
    pub values: Vec<SymbolId>,
    /// Those its methods use, which need fields.
    pub in_methods: Vec<SymbolId>,
    /// Whether its methods use them all, which names their fields before the parameters.
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
    let span = l.tpd.span(td);
    let parents: Vec<SymbolId> = l.u.parents(class).into_iter().filter_map(|p| l.u.class_symbol(p)).collect();
    let refused = match () {
        _ if is_module_class(l.u, class) => Some("local objects"),
        _ if has(l.u, class, CASE) => Some("local case classes"),
        _ if is_trait(l.u, class) => Some("local traits"),
        _ if !l.u.type_params(class).is_empty() => Some("generic local classes"),
        _ if parents.iter().any(|&c| is_local(l, c)) => Some("local classes extending local classes"),
        _ => None,
    };
    if let Some(what) = refused {
        return unsupported(span, what);
    }
    let l: &Lower = l;
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
                if local && l.u.symbol(s).kind == Kind::Def {
                    return unsupported(span, "local classes calling local methods");
                }
                if !local || !matches!(l.u.symbol(s).kind, Kind::Val | Kind::Param) {
                    continue;
                }
                if has(l.u, s, MUTABLE) {
                    return unsupported(span, "local classes capturing variables");
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
    free.values.sort_by_key(|s| s.0);
    free.in_methods.sort_by_key(|s| s.0);
    free.methods_first = free.values.len() == free.in_methods.len();
    Ok(free)
}

/// Whether `s` is a method whose body runs outside the constructor: not a constructor, nor a
/// field's getter, whose body is its initializer.
pub fn is_method(l: &Lower, s: Option<SymbolId>) -> bool {
    s.is_some_and(|s| {
        !crate::symbols::is_constructor(l.u, s) && (!has(l.u, s, FIELDaccessor) || has(l.u, s, LAZY))
    })
}
