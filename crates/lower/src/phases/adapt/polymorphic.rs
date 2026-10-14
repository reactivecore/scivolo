//! Calls whose erased signature is not their method's: the operations of arrays of a known
//! element type and `synchronized`.

use crate::{Lower, Result, unsupported};
use tasty::{Erased, Type};
use typer::tpd::{TNodeId, Tree};

/// The erased parameters and result of array operation `fun`, a selection of `apply`, `update`
/// or `length` on an array of a known element type; `None` for another function.
pub fn array_op(l: &mut Lower, fun: TNodeId) -> Option<(Vec<Erased>, Erased)> {
    let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { return None };
    let m = l.tpd.sym(fun)?;
    let owner = l.u.symbol(m).owner?;
    if l.u.full_name(owner) != "scala.Array" {
        return None;
    }
    let array = l.u.widen(l.tpd.tpe(qualifier));
    let Erased::Array(elem) = l.u.erase(array) else { return None };
    let int = Erased::Prim('i');
    match l.u.symbol(m).name.as_str() {
        "apply" => Some((vec![int], *elem)),
        "update" => Some((vec![int, *elem], Erased::Unit)),
        "length" => Some((Vec::new(), int)),
        _ => None,
    }
}

/// Whether `fun` applies `synchronized` to a type, which stays polymorphic after erasure.
pub fn is_synchronized(l: &Lower, fun: TNodeId) -> bool {
    matches!(*l.tpd.tree(fun),
        Tree::TypeApply { fun: sel, .. } if l.tpd.sym(sel) == Some(l.defs.synchronized))
}

/// The erased type argument of `synchronized` application `fun`, `BoxedUnit` for `Unit`; `None`
/// for another function.
pub fn synchronized_type(l: &mut Lower, fun: TNodeId) -> Option<Erased> {
    let Tree::TypeApply { args, .. } = *l.tpd.tree(fun) else { return None };
    let &[targ] = l.tpd.items(args) else { return None };
    is_synchronized(l, fun).then(|| match l.u.erase(l.tpd.tpe(targ)) {
        Erased::Unit => Erased::Class("scala.runtime.BoxedUnit".into()),
        e => e,
    })
}

/// Makes identifier `sel` of `synchronized` its selection on the `this` of its type's prefix, as
/// nscplugin's `DesugaredSelect` sees it.
pub fn qualify_synchronized(l: &mut Lower, sel: TNodeId) -> Result<()> {
    let Tree::Ident { name } = *l.tpd.tree(sel) else { return Ok(()) };
    let span = l.tpd.span(sel);
    let class = match *l.u.types().get(l.tpd.tpe(sel)) {
        Type::Sym(Some(p), _) | Type::TermRef(p, _) => match *l.u.types().get(p) {
            Type::This(c) => l.u.class_symbol(c),
            _ => None,
        },
        _ => None,
    };
    let Some(class) = class else { return unsupported(span, "`synchronized` on a prefix other than `this`") };
    let qualifier = l.this(class, span);
    l.tpd.get_mut(sel).tree = Tree::Select { qualifier, name };
    Ok(())
}
