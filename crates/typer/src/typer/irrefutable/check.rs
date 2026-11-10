// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/patmat/Space.scala
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//! Whether a pattern matches every value of its selector's type (dotty's
//! `Checking.checkIrrefutable`, `Space.isIrrefutable`).

use super::super::Typer;
use super::super::extractor_members::{
    NON_EMPTY_TUPLE, PRODUCT, extractor_member, product_selectors, seq_elem_type,
};
use super::super::unapply_args::UnapplyArgs;
use super::{RUNTIME_CHECKED, Refutable, UNCHECKED, has_annotation};
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Const, Type, TypeId, Universe, constant};

/// Finds the first reason dotty's `checkIrrefutable` finds why pattern `pat` may not match a
/// value of type `pt`.
pub fn check_irrefutable(t: &mut Typer<'_>, pat: TNodeId, pt: TypeId) -> Result<(), Refutable> {
    if has_annotation(t.u, pt, UNCHECKED) || has_annotation(t.u, pt, RUNTIME_CHECKED) {
        return Ok(());
    }
    match *t.tpd.tree(pat) {
        T::Bind { body, .. } | T::NamedArg { arg: body, .. } => check_irrefutable(t, body, pt),
        T::UnApply { fun, patterns, .. } => {
            check_conforms(t, pat, pt)?;
            let pats = t.tpd.items(patterns).to_vec();
            if !is_irrefutable_extractor(t, fun, pats.len()) {
                return Err(Refutable::Extractor(pat));
            }
            let arg_types = extractor_arg_types(t, fun, pats.len()).ok_or(Refutable::Unknown)?;
            pats.into_iter().zip(arg_types).try_for_each(|(p, a)| check_irrefutable(t, p, a))
        }
        T::Alternative { trees } => {
            let alts = t.tpd.items(trees).to_vec();
            alts.into_iter().try_for_each(|a| check_irrefutable(t, a, pt))
        }
        T::Typed { expr, .. } => {
            check_conforms(t, pat, pt)?;
            check_irrefutable(t, expr, pt)
        }
        T::Ident { name } if t.text(name.name) == "_" => Ok(()),
        _ => check_conforms(t, pat, pt),
    }
}

/// Whether `pt` stripped of names, or a type it widens to, conforms to the type of pattern
/// `pat`.
fn check_conforms(t: &mut Typer<'_>, pat: TNodeId, pt: TypeId) -> Result<(), Refutable> {
    let pat_tpe = t.tpd.tpe(pat);
    let mut tpe = t.strip_named_tuple(pt);
    loop {
        if t.u.is_subtype(tpe, pat_tpe) {
            return Ok(());
        }
        let wide = t.u.widen(tpe);
        if wide == tpe {
            return Err(Refutable::NonConforming(pat, pt));
        }
        tpe = wide;
    }
}

/// Whether extractor `fun` with `n` patterns matches every value its parameter accepts
/// (dotty's `Space.isIrrefutable`).
fn is_irrefutable_extractor(t: &mut Typer<'_>, fun: TNodeId, n: usize) -> bool {
    let result = extractor_result(t, fun);
    let sym = t.tpd.sym(t.tpd.meth_part(fun));
    let case_companion = sym.is_some_and(|s| {
        let synthetic = t.u.symbol(s).is_synthetic();
        let owner = t.u.symbol(s).owner;
        let class = owner.and_then(|o| t.u.companion_class(o));
        synthetic && class.is_some_and(|c| t.u.symbol(c).is_case())
    });
    let some = t.resolve_full_name("scala.Some", true);
    t.u.class_symbol(result).is_some_and(|c| Some(c) == some)
        || is_bool_constant(t.u, result, true)
        || case_companion
        || seq_elem_type(t, result).is_some()
        || n > 0
            && t.derives_from_named(result, PRODUCT)
            && product_selectors(t, result).is_ok_and(|s| s.len() == n)
        || extractor_member(t, result, "isEmpty")
            .ok()
            .flatten()
            .is_some_and(|e| is_bool_constant(t.u, e, false))
        || t.derives_from_named(result, NON_EMPTY_TUPLE)
}

/// The result type of extractor `fun`.
fn extractor_result(t: &mut Typer<'_>, fun: TNodeId) -> TypeId {
    let fun_tpe = t.tpd.tpe(fun);
    let info = t.method_info(fun_tpe).unwrap_or(fun_tpe);
    let result = t.final_result(info);
    t.zonk(result)
}

/// Whether `tpe` is the constant type `true` or `false` of `value`.
fn is_bool_constant(u: &mut Universe, tpe: TypeId, value: bool) -> bool {
    let tpe = u.dealias(tpe);
    matches!(*u.types().get(tpe), Type::Constant(tag, bits) if constant(tag, bits) == Some(Const::Bool(value)))
}

/// The types extractor `fun` gives its `n` patterns (dotty's `UnapplyArgs.argTypes`).
fn extractor_arg_types(t: &mut Typer<'_>, fun: TNodeId, n: usize) -> Option<Vec<TypeId>> {
    let result = extractor_result(t, fun);
    let seq = t.tpd.sym(t.tpd.meth_part(fun)).is_some_and(|s| t.u.symbol(s).name == "unapplySeq");
    let args = UnapplyArgs { result, n };
    if !seq {
        return args.arg_types(t).ok();
    }
    args.seq_arg_types(t).ok().map(|args| args.types)
}
