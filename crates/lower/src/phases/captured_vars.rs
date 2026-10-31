// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/CapturedVars.scala
//! Local variables captured by closures (dotty's `CapturedVars`): `var x = e` becomes
//! `val x = XRef.create(e)`, a read `x.elem` and a write `x.elem = e`, so closures share the
//! variable.

use super::adapt::ascribed;
use super::walk::subtrees;
use crate::definitions::{Ref, ref_class, type_ref};
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::Span;
use tasty::tags::{FIELDaccessor, LAZY, MUTABLE};
use tasty::{Erased, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree, Walk, traverse};

/// Makes the variables in class definition `td` that closures capture shared references.
pub fn transform(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let mut captured = FxHashSet::default();
    collect(l, td, class, &mut captured);
    if captured.is_empty() {
        return Ok(());
    }
    let mut refs = FxHashMap::default();
    for &v in &captured {
        let info = l.u.info(v);
        let erased = l.u.erase(info);
        let Ok(r) = ref_class(l.u, &erased) else {
            return unsupported(syntax::ast::Span::NONE, "captured variables of this type");
        };
        l.fields.insert(r.elem, r.elem);
        refs.insert(v, r);
    }
    rewrite(l, td, &refs);
    for (&v, &r) in &refs {
        let flags = l.u.symbol(v).flags.without(MUTABLE);
        l.u.set_flags(v, flags);
        let tpe = type_ref(l.u, r.class);
        l.u.set_info(v, tpe);
    }
    Ok(())
}

/// The local variables referred to under `t`, in class `class`, from inside a function, class or
/// lazy value other than their own function, a value's being that of the function it is in; the
/// class body and its field initializers count as the constructor.
fn collect(l: &mut Lower, t: TNodeId, class: SymbolId, out: &mut FxHashSet<SymbolId>) {
    let body = match *l.tpd.tree(t) {
        Tree::TypeDef { rhs, .. } => l.tpd.sym(rhs),
        _ => None,
    };
    let initializer = |l: &Lower, f: SymbolId| {
        let field = !has(l.u, f, LAZY) && l.u.symbol(f).owner == Some(class);
        f == class
            || Some(f) == body
            || field && (has(l.u, f, FIELDaccessor) || l.u.symbol(f).kind == Kind::Val)
    };
    let enclosing = |l: &Lower, mut o: SymbolId| {
        while l.u.symbol(o).kind == Kind::Val && !has(l.u, o, LAZY) && !initializer(l, o) {
            o = l.u.symbol(o).owner?;
        }
        Some(o)
    };
    traverse(l, t, class, &mut |l, t, function| {
        if let (Tree::Ident { .. }, Some(s)) = (l.tpd.tree(t), l.tpd.sym(t))
            && has(l.u, s, MUTABLE)
            && let Some(owner) = l.u.symbol(s).owner.and_then(|o| enclosing(l, o))
            && (l.u.symbol(owner).kind == Kind::Def || has(l.u, owner, LAZY) || initializer(l, owner))
            && !l.u.is_class(owner)
            && owner != function
            && !(initializer(l, function)
                && (initializer(l, owner)
                    || l.u.is_constructor(owner) && l.u.symbol(owner).owner == Some(class)))
        {
            out.insert(s);
        }
        match (l.tpd.tree(t), l.tpd.sym(t)) {
            (Tree::ValDef { .. } | Tree::DefDef { .. }, Some(v)) if l.lazy_members.contains_key(&v) => {
                Walk::Within(l.lazy_members[&v].init)
            }
            (Tree::ValDef { .. }, Some(v)) if has(l.u, v, LAZY) => Walk::Children,
            (Tree::ValDef { .. }, _) => Walk::Within(function),
            _ => Walk::Children,
        }
    });
}

/// Rewrites the definitions, reads and writes of the variables of `refs` in class definition
/// `td`; the values stored and read through a reference of objects typed as the variable.
fn rewrite(l: &mut Lower, td: TNodeId, refs: &FxHashMap<SymbolId, Ref>) {
    let captured = |l: &Lower, t| l.tpd.sym(t).is_some_and(|s| refs.contains_key(&s));
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let written: FxHashSet<TNodeId> = subtrees(l.tpd, td)
        .into_iter()
        .filter_map(|t| match *l.tpd.tree(t) {
            Tree::Assign { lhs, .. } if captured(l, lhs) => Some(lhs),
            _ => None,
        })
        .collect();
    traverse(l, td, class, &mut |l, t, _| match *l.tpd.tree(t) {
        Tree::ValDef { name, rhs: Some(rhs), .. } if captured(l, t) => {
            let v = l.tpd.sym(t).expect("a variable");
            let r = refs[&v];
            let span = l.tpd.span(rhs);
            let class = l.ident(r.class, Span::NONE);
            let create = l.select(class, r.create, Span::NONE);
            let tpe = type_ref(l.u, r.class);
            let value = as_variable(l, rhs, v, r);
            let rhs = l.apply(create, &[value], tpe, Span::NONE);
            l.positioned(rhs);
            let tpt = l.type_tree(tpe, span);
            l.tpd.get_mut(t).tree = Tree::ValDef { name, tpt, rhs: Some(rhs) };
            Walk::Children
        }
        Tree::Assign { lhs, rhs } if written.contains(&lhs) => {
            let v = l.tpd.sym(lhs).expect("a variable");
            let rhs = as_variable(l, rhs, v, refs[&v]);
            l.tpd.get_mut(t).tree = Tree::Assign { lhs, rhs };
            Walk::Children
        }
        Tree::Ident { .. } if captured(l, t) => {
            to_elem(l, t, refs, !written.contains(&t));
            Walk::Skip
        }
        _ => Walk::Children,
    });
}

/// Makes reference `t` to variable `x` the selection `x.elem`, `x.elem: T` if `read` for `x` of
/// type `T` held by a reference of objects (dotty's `ensureConforms`).
fn to_elem(l: &mut Lower, t: TNodeId, refs: &FxHashMap<SymbolId, Ref>, read: bool) {
    let v = l.tpd.sym(t).expect("a variable");
    let r = refs[&v];
    let info = l.u.info(r.elem);
    let span = l.tpd.span(t);
    let var = l.ident(v, span);
    let elem_name = l.term_name("elem");
    let sel = l.tpd.node(Tree::Select { qualifier: var, name: elem_name }, span, Some(info), Some(r.elem));
    let elem = if read { as_variable(l, sel, v, r) } else { sel };
    l.copy_node(t, elem);
}

/// `e: T` for variable `v` of type `T` if its reference `r` holds objects and `T` erases to
/// another class or an array; else `e`.
fn as_variable(l: &mut Lower, e: TNodeId, v: SymbolId, r: Ref) -> TNodeId {
    let tpe = l.u.info(v);
    let elem = l.u.info(r.elem);
    let object = |x: &Erased| matches!(x, Erased::Class(c) if c == "java.lang.Object");
    let objects = object(&l.u.erase(elem));
    let erased = l.u.erase(tpe);
    if !objects || object(&erased) || l.u.value_class(tpe).is_some() {
        return e;
    }
    let span = l.tpd.span(e);
    ascribed(l, e, tpe, span)
}
