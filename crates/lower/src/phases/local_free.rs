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
    if is_static(l.u, outer) {
        return outer;
    }
    let mut c = outer;
    while let Some(o) = l.u.symbol(c).owner.filter(|&o| l.u.symbol(o).kind != Kind::Package) {
        c = o;
    }
    c
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
    let refused = match () {
        _ if l.u.is_trait(class) => Some("local traits"),
        _ if !l.u.type_params(class).is_empty() => Some("generic local classes"),
        _ => nested_definition(l, td),
    };
    if let Some(what) = refused {
        return unsupported(span, what);
    }
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
        let type_tree = matches!(
            l.tpd.tree(t),
            Tree::SingletonTypeTree { .. } | Tree::AppliedTypeTree { .. } | Tree::TypeTree { .. }
        );
        if !type_tree {
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
                if owner == Some(outer) && !static_outer && !l.u.is_class(s) {
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
    let nested = |t: TNodeId| match *l.tpd.tree(t) {
        Tree::TypeDef { rhs, .. } => t != td && matches!(l.tpd.tree(rhs), Tree::Template { .. }),
        _ => false,
    };
    let free_ref = |t: TNodeId| {
        matches!(l.tpd.tree(t), Tree::Ident { .. })
            && l.tpd.sym(t).is_some_and(|s| free.values.contains(&s) || free.in_methods.contains(&s))
    };
    let outside_call = |t: TNodeId| {
        let Some(s) = l.tpd.sym(t).filter(|_| matches!(l.tpd.tree(t), Tree::Ident { .. })) else {
            return false;
        };
        let owner = l.u.symbol(s).owner;
        l.u.symbol(s).kind == Kind::Def && owner.is_some_and(|o| is_term(l, o) && !within(l, o, class))
    };
    for c in super::explicit_outer::terms(l, td, true).into_iter().filter(|&t| nested(t)) {
        let terms = super::explicit_outer::terms(l, c, true);
        if let Some(&t) = terms.iter().find(|&&t| free_ref(t)) {
            return unsupported(l.tpd.span(t), "classes in local classes capturing values");
        }
        if let Some(&t) = terms.iter().find(|&&t| outside_call(t)) {
            return unsupported(l.tpd.span(t), "classes in local classes calling local methods");
        }
    }
    Ok(free)
}

/// What local class definition `td` defines that the lowering does not lift with it: lambdas and
/// local methods, lazy values of an object.
fn nested_definition(l: &Lower, td: TNodeId) -> Option<&'static str> {
    super::explicit_outer::terms(l, td, true).into_iter().find_map(|t| match *l.tpd.tree(t) {
        Tree::Closure { .. } => Some("lambdas and local methods in local classes"),
        Tree::ValDef { .. } | Tree::DefDef { .. }
            if l.tpd.sym(t).is_some_and(|s| {
                has(l.u, s, LAZY) && !l.u.is_module(s) && l.u.symbol(s).owner == l.tpd.sym(td)
            }) && l.tpd.sym(td).is_some_and(|c| l.u.is_module_class(c)) =>
        {
            Some("lazy values in local objects")
        }
        Tree::DefDef { .. }
            if l.tpd.sym(t).is_some_and(|s| is_term(l, l.u.symbol(s).owner.expect("an owner"))) =>
        {
            Some("lambdas and local methods in local classes")
        }
        _ => None,
    })
}

/// Whether `s` is a method whose body runs outside the constructor: not a constructor, nor a
/// field's getter, whose body is its initializer.
pub fn is_method(l: &Lower, s: Option<SymbolId>) -> bool {
    s.is_some_and(|s| !l.u.is_constructor(s) && (!has(l.u, s, FIELDaccessor) || has(l.u, s, LAZY)))
}
