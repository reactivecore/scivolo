//! dotty's `ExplicitSelf`: a reference to a class's self becomes `C.this`, and a member `m` of
//! `C.this` that `C` does not inherit is selected on `C.this.asInstanceOf[S & C.this.type]`, `S`
//! the self type of `C`.

use super::walk::walk_class;
use crate::Lower;
use syntax::ast::Span;
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Tree, Walk};

pub fn transform(l: &mut Lower, td: TNodeId) {
    walk_class(l, td, &mut |l, t, _| {
        rewrite(l, t);
        Walk::Children
    });
}

fn rewrite(l: &mut Lower, t: TNodeId) {
    let span = l.tpd.span(t);
    match *l.tpd.tree(t) {
        Tree::Ident { name } if !name.is_type => {
            let Some(tpe) = l.tpd.get(t).tpe else { return };
            match *l.u.types().get(tpe) {
                Type::This(c) => {
                    let Some(c) = l.u.class_symbol(c) else { return };
                    let this = l.this(c, span);
                    l.copy_node(t, this);
                }
                Type::Sym(Some(p), _) => {
                    let Type::This(c) = *l.u.types().get(p) else { return };
                    let Some(c) = l.u.class_symbol(c).filter(|&c| needs_cast(l, t, c)) else { return };
                    let this = l.this(c, Span::NONE);
                    let qualifier = cast(l, this, c);
                    let n = l.tpd.get(t);
                    let (tpe, sym) = (n.tpe, n.sym);
                    let select = l.tpd.node(Tree::Select { qualifier, name }, span, tpe, sym);
                    l.copy_node(t, select);
                }
                _ => {}
            }
        }
        Tree::Select { qualifier, name } if !name.is_type => {
            let (Tree::This { .. }, Some(c)) = (l.tpd.tree(qualifier), l.tpd.sym(qualifier)) else { return };
            if needs_cast(l, t, c) {
                let qualifier = cast(l, qualifier, c);
                l.tpd.get_mut(t).tree = Tree::Select { qualifier, name };
            }
        }
        _ => {}
    }
}

/// Whether reference `t` to a member of `C.this`, `c` the class `C`, needs its qualifier cast.
fn needs_cast(l: &mut Lower, t: TNodeId, c: SymbolId) -> bool {
    let owner = l.tpd.sym(t).and_then(|s| l.u.symbol(s).owner);
    l.u.given_self_type(c).is_some() && owner.is_some_and(|o| !l.u.derives_from(c, o))
}

/// `this.asInstanceOf[S & C.this.type]` of `this`, `C.this` of class `c`.
fn cast(l: &mut Lower, this: TNodeId, c: SymbolId) -> TNodeId {
    let self_type = l.u.self_type(c);
    let this_type = l.tpd.tpe(this);
    let target = l.u.intern_type(Type::And(self_type, this_type));
    let m = l.defs.as_instance_of;
    let fun = l.select(this, m, Span::NONE);
    let fun = l.positioned(fun);
    let tpt = l.type_tree(target, Span::NONE);
    let args = l.tpd.list(&[tpt]);
    let t = l.tpd.node(Tree::TypeApply { fun, args }, Span::NONE, Some(target), Some(m));
    l.positioned(t)
}
