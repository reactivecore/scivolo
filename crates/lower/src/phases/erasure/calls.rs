// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! Calls typed by `Erasure`: the receiver adapted to the class of the called member, each
//! argument typed against its erased parameter type.

use super::generic_arrays::runtime_op;
use super::{typed, typed_qualifier};
use crate::definitions::type_ref;
use crate::phases::adapt::{array_op, from_start, is_synchronized, qualify_synchronized, synchronized_type};
use crate::phases::casts::{erased_function_class, is_cast};
use crate::{Lower, Result};
use syntax::ast::Span;
use tasty::{Erased, SymbolId, Type};
use typer::tpd::{List, TNodeId, Tree};

/// Call `t` of `fun` to `args`, its argument lists flattened and its type arguments dropped; at
/// the span of the innermost call that trailing empty argument lists apply.
pub fn typed_apply(l: &mut Lower, t: TNodeId, mut fun: TNodeId, args: List) -> Result<Tree> {
    let mut last = (t, args, fun);
    while l.tpd.items(last.1).is_empty()
        && let Tree::Apply { fun: inner, args } = *l.tpd.tree(last.2)
    {
        last = (last.2, args, inner);
    }
    l.tpd.get_mut(t).span = l.tpd.span(last.0);
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
    if let Some((op, qualifier)) = runtime_op(l, fun)? {
        if all.is_empty() {
            l.tpd.get_mut(t).span = from_start(l.tpd.span(t));
        }
        all.insert(0, qualifier);
        fun = op;
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
    let any_val_init = l.tpd.sym(sel).filter(|&m| l.u.is_constructor(m)).and_then(|m| l.u.symbol(m).owner);
    if any_val_init == Some(l.defs.any_val) {
        let init = l.u.decl(l.defs.object, "<init>").unwrap_or_default();
        l.tpd.get_mut(sel).sym = init.first().copied();
    }
    let clone = l.tpd.sym(fun).filter(|&m| l.u.symbol(m).name == "clone");
    if clone.is_some() && clone == Some(l.array_clone()?) {
        l.tpd.get_mut(fun).sym = Some(l.defs.object_clone);
    }
    let params = match (array_op(l, fun), synchronized_type(l, fun)) {
        (Some((params, _)), _) => params,
        (None, Some(e)) => vec![e],
        (None, None) => l.tpd.sym(fun).map(|m| l.u.erased_method_value(m).0).unwrap_or_default(),
    };
    if is_bunched(&params, all.len()) {
        all = vec![bunched_args(l, &all)];
    }
    let mut typed_args = Vec::new();
    for (i, &a) in all.iter().enumerate() {
        typed_args.push(typed(l, a, params.get(i))?);
    }
    Ok(Tree::Apply { fun, args: l.tpd.list(&typed_args) })
}

/// Whether `n` arguments go to erased parameters `params` as one array: a `FunctionXXL`'s
/// `apply` called with more arguments than the library's arities (dotty's `expandedMethodType`).
fn is_bunched(params: &[Erased], n: usize) -> bool {
    let [Erased::Array(elem)] = params else { return false };
    n > tasty::MAX_IMPLEMENTED_ARITY && matches!(&**elem, Erased::Class(c) if c == "java.lang.Object")
}

/// `args` as the elements of an array of objects (dotty's `JavaSeqLiteral`).
fn bunched_args(l: &mut Lower, args: &[TNodeId]) -> TNodeId {
    let span = args.iter().fold(Span::NONE, |s, &a| s.union(l.tpd.span(a)));
    let object = type_ref(l.u, l.defs.object);
    let elemtpt = l.type_tree(object, Span::NONE);
    let array = l.u.class_ref("scala.Array");
    let tpe = l.u.intern_type(Type::Applied(array, vec![object]));
    let elems = l.tpd.list(args);
    l.tpd.node(Tree::SeqLiteral { elems, elemtpt }, span, Some(tpe), None)
}

/// The member of `Any` that the JDK's `java.lang.Class` inherits where nativelib's `_Class`
/// overrides `m`: `equals`, `hashCode`.
pub fn inherited_by_class(l: &mut Lower, m: SymbolId) -> Option<SymbolId> {
    let s = l.u.symbol(m);
    if !matches!(s.name.as_str(), "equals" | "hashCode") || l.u.binary_name(s.owner?) != "java.lang.Class" {
        return None;
    }
    let name = s.name.clone();
    l.u.decl(l.defs.any, &name).unwrap_or_default().first().copied()
}

/// The member of `FunctionN` or `FunctionXXL` that member `m` of `ContextFunctionN` or of a
/// `FunctionN` above the library's arities becomes (dotty's `Erasure.Typer.mapOwner`).
pub fn function_member(l: &mut Lower, m: SymbolId) -> Option<SymbolId> {
    let owner = l.u.symbol(m).owner?;
    let erased = erased_function_class(l, owner).filter(|&c| c != owner)?;
    let name = l.u.symbol(m).name.clone();
    l.u.decl(erased, &name).unwrap_or_default().first().copied()
}

/// Whether `t` selects `Any.hashCode` without applying it, as `inherited_by_class` leaves
/// `_Class`'s parameterless `hashCode`.
pub fn is_unapplied_hash_code(l: &Lower, t: TNodeId) -> bool {
    let any_hash_code = |m| l.u.symbol(m).name == "hashCode" && l.u.symbol(m).owner == Some(l.defs.any);
    matches!(l.tpd.tree(t), Tree::Select { .. }) && l.tpd.sym(t).is_some_and(any_hash_code)
}
