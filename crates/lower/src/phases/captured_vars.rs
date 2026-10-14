//! Local variables captured by closures (dotty's `CapturedVars`): `var x = e` becomes
//! `val x = XRef.create(e)`, a read `x.elem` and a write `x.elem = e`, so closures share the
//! variable.

use crate::definitions::{Ref, ref_class, type_ref};
use crate::symbols::has;
use crate::{Lower, Result, unsupported};
use rustc_hash::{FxHashMap, FxHashSet};
use tasty::tags::{LAZY, MUTABLE};
use tasty::{Kind, SymbolId};
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
/// lazy value other than their own function; the class body counts as the constructor.
fn collect(l: &mut Lower, t: TNodeId, class: SymbolId, out: &mut FxHashSet<SymbolId>) {
    let body = match *l.tpd.tree(t) {
        Tree::TypeDef { rhs, .. } => l.tpd.sym(rhs),
        _ => None,
    };
    traverse(l, t, class, &mut |l, t, function| {
        if let (Tree::Ident { .. }, Some(s)) = (l.tpd.tree(t), l.tpd.sym(t))
            && has(l.u, s, MUTABLE)
            && let Some(owner) = l.u.symbol(s).owner
            && (l.u.symbol(owner).kind == Kind::Def || has(l.u, owner, LAZY))
            && !l.u.is_class(owner)
            && owner != function
            && !(Some(function) == body
                && l.u.is_constructor(owner)
                && l.u.symbol(owner).owner == Some(class))
        {
            out.insert(s);
        }
        match (l.tpd.tree(t), l.tpd.sym(t)) {
            (Tree::ValDef { .. }, Some(v)) if has(l.u, v, LAZY) => Walk::Children,
            (Tree::ValDef { .. }, _) => Walk::Within(function),
            _ => Walk::Children,
        }
    });
}

/// Rewrites the definitions, reads and writes of the variables of `refs` in class definition
/// `td`.
fn rewrite(l: &mut Lower, td: TNodeId, refs: &FxHashMap<SymbolId, Ref>) {
    let captured = |l: &Lower, t| l.tpd.sym(t).is_some_and(|s| refs.contains_key(&s));
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    traverse(l, td, class, &mut |l, t, _| match *l.tpd.tree(t) {
        Tree::ValDef { name, rhs: Some(rhs), .. } if captured(l, t) => {
            let r = refs[&l.tpd.sym(t).expect("a variable")];
            let span = l.tpd.span(rhs);
            let class = l.ident(r.class, span);
            let create = l.select(class, r.create, span);
            let tpe = type_ref(l.u, r.class);
            let rhs = l.apply(create, &[rhs], tpe, span);
            let tpt = l.type_tree(tpe, span);
            l.tpd.get_mut(t).tree = Tree::ValDef { name, tpt, rhs: Some(rhs) };
            Walk::Children
        }
        Tree::Ident { .. } if captured(l, t) => {
            to_elem(l, t, refs);
            Walk::Skip
        }
        _ => Walk::Children,
    });
}

/// Makes reference `t` to variable `x` the selection `x.elem`.
fn to_elem(l: &mut Lower, t: TNodeId, refs: &FxHashMap<SymbolId, Ref>) {
    let v = l.tpd.sym(t).expect("a variable");
    let r = refs[&v];
    let info = l.u.info(r.elem);
    let span = l.tpd.span(t);
    let var = l.ident(v, span);
    let elem_name = l.term_name("elem");
    let node = l.tpd.get_mut(t);
    (node.tree, node.sym, node.tpe) =
        (Tree::Select { qualifier: var, name: elem_name }, Some(r.elem), Some(info));
}
