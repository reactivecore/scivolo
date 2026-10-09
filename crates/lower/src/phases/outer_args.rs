//! The outer arguments of dotty's `ExplicitOuter`: `new p.C(args)` of a class `C` taking its
//! enclosing instance becomes `new p.C(p, args)`, `this` for a prefix `Outer.this`.

use super::explicit_outer::{enclosing_instance, terms};
use crate::{Lower, Result, unsupported};
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// Passes the enclosing instance to each `new C` in class definition `td` of a class `C` that
/// takes it.
pub fn pass_outer(l: &mut Lower, td: TNodeId) -> Result<()> {
    let class = l.tpd.sym(td).expect("a class definition's symbol");
    let parents = match *l.tpd.tree(td) {
        Tree::TypeDef { rhs, .. } => match *l.tpd.tree(rhs) {
            Tree::Template { parents, .. } => l.tpd.items(parents).to_vec(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
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
        if parents.contains(&t) && enclosing_instance(l, created).is_some() {
            return unsupported(span, "subclasses of classes in classes");
        }
        if !l.outers.contains_key(&created) {
            continue;
        }
        let at = Span::at(l.tpd.span(ctor).start());
        let instance = instance(l, class, tpe, at)?;
        let all: Vec<TNodeId> = std::iter::once(instance).chain(l.tpd.items(args).iter().copied()).collect();
        let args = l.tpd.list(&all);
        l.tpd.get_mut(t).tree = Tree::Apply { fun, args };
    }
    Ok(())
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

/// Stable path `p` as a tree in `class`: `this` of `class`, a local value or parameter, or a
/// value of `class`.
fn path(l: &mut Lower, class: SymbolId, p: TypeId, span: Span) -> Result<TNodeId> {
    match l.u.types().get(p).clone() {
        Type::This(c) if l.u.class_symbol(c) == Some(class) => Ok(l.this(class, Span::NONE)),
        Type::Sym(_, s) if matches!(l.u.symbol(s).kind, Kind::Val | Kind::Param) => {
            let owner = l.u.symbol(s).owner;
            if owner.is_some_and(|o| l.u.symbol(o).kind == Kind::Def) {
                Ok(l.ident(s, span))
            } else if owner == Some(class) {
                let this = l.this(class, span);
                Ok(l.select(this, s, span))
            } else {
                unsupported(span, "creating an inner class through this prefix")
            }
        }
        _ => unsupported(span, "creating an inner class through this prefix"),
    }
}
