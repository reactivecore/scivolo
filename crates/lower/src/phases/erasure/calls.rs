//! Calls typed by `Erasure`: the receiver adapted to the class of the called member, each
//! argument typed against its erased parameter type.

use super::{typed, typed_qualifier};
use crate::phases::adapt::{array_op, erased_type, is_synchronized, qualify_synchronized, synchronized_type};
use crate::phases::casts::is_cast;
use crate::{Lower, Result, unsupported};
use tasty::Erased;
use typer::tpd::{List, TNodeId, Tree};

/// Call `t` of `fun` to `args`, its argument lists flattened and its type arguments dropped.
pub fn typed_apply(l: &mut Lower, t: TNodeId, mut fun: TNodeId, args: List) -> Result<Tree> {
    let mut all = l.tpd.items(args).to_vec();
    loop {
        match *l.tpd.tree(fun) {
            Tree::Apply { fun: inner, args } => {
                all.splice(0..0, l.tpd.items(args).iter().copied());
                fun = inner;
            }
            Tree::TypeApply { fun: inner, .. } if !is_cast(l, fun) && !is_synchronized(l, fun) => fun = inner,
            _ => break,
        }
    }
    let sel = match *l.tpd.tree(fun) {
        Tree::TypeApply { fun: inner, .. } if is_synchronized(l, fun) => {
            qualify_synchronized(l, inner)?;
            inner
        }
        _ => fun,
    };
    if let Tree::Select { qualifier, name } = *l.tpd.tree(sel) {
        let qualifier = typed_qualifier(l, sel, qualifier)?;
        l.tpd.get_mut(sel).tree = Tree::Select { qualifier, name };
    }
    if l.tpd.sym(fun) == Some(l.defs.array_clone) {
        let Tree::Select { qualifier, .. } = *l.tpd.tree(fun) else { unreachable!("a selection") };
        if !matches!(erased_type(l, qualifier), Erased::Array(_)) {
            return unsupported(l.tpd.span(t), "`clone` of a generic array");
        }
        l.tpd.get_mut(fun).sym = Some(l.defs.object_clone);
    }
    let params = match (array_op(l, fun), synchronized_type(l, fun)) {
        (Some((params, _)), _) => params,
        (None, Some(e)) => vec![e],
        (None, None) => l.tpd.sym(fun).map(|m| l.u.erased_method(m).0).unwrap_or_default(),
    };
    let mut typed_args = Vec::new();
    for (i, &a) in all.iter().enumerate() {
        typed_args.push(typed(l, a, params.get(i))?);
    }
    Ok(Tree::Apply { fun, args: l.tpd.list(&typed_args) })
}
