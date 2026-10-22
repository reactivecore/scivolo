// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala
//! Lambdas of more parameters than the library's function arities: their methods take the
//! arguments as one array `$allArgs`, which the body unpacks into the parameters.

use super::casts::class_named;
use super::lambda_lift::Function;
use super::lazy_trees::{local, method};
use super::lifted::substitute;
use crate::definitions::{symbol, type_ref};
use crate::{Lower, Result, unsupported};
use rustc_hash::FxHashMap;
use syntax::ast::Span;
use tasty::{Flags, Kind, MAX_IMPLEMENTED_ARITY, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The name of the array parameter (dotty's `nme.ALLARGS`).
const ALL_ARGS: &str = "$allArgs";

/// Whether the method of function `f` of `n` parameters takes them as one array; `Err` for a
/// lambda of that many parameters that is not a closure.
pub fn is_bunched(l: &Lower, f: &Function, n: usize) -> Result<bool> {
    if n <= MAX_IMPLEMENTED_ARITY || !f.is_anonymous(l) {
        return Ok(false);
    }
    if f.closure.is_none() {
        return unsupported(l.tpd.span(f.def), "lambdas of more than 22 parameters outside closures");
    }
    Ok(true)
}

/// The parameter `$allArgs: Array[Object]` by name and type.
pub fn param(l: &mut Lower) -> (String, TypeId) {
    let object = type_ref(l.u, l.defs.object);
    let array = l.u.class_ref("scala.Array");
    (ALL_ARGS.to_owned(), l.u.intern_type(Type::Applied(array, vec![object])))
}

/// The symbol of parameter `$allArgs` of `method`.
pub fn define(l: &mut Lower, method: SymbolId) -> SymbolId {
    let (name, info) = param(l);
    let s = l.u.define(symbol(&name, Kind::Param, method, Flags::default()));
    l.u.set_info(s, info);
    s
}

/// `rhs` of `method` after the parameters `params` of its lambda become values of `method` holding
/// the elements of `all_args` (dotty's `Erasure.Typer.typedDefDef`).
pub fn unpacked(
    l: &mut Lower,
    (owner, all_args): (SymbolId, SymbolId),
    params: &[TNodeId],
    rhs: TNodeId,
) -> Result<TNodeId> {
    let array = class_named(l, "scala.Array").expect("`scala.Array`");
    let apply = method(l, array, "apply", Span::NONE)?;
    let object = type_ref(l.u, l.defs.object);
    let mut map = FxHashMap::default();
    let mut defs = Vec::new();
    for (i, &p) in params.iter().enumerate() {
        let sym = l.tpd.sym(p).expect("a parameter's symbol");
        let name = l.u.symbol(sym).name.clone();
        let info = l.u.info(sym);
        let value = local(l, owner, &name, info, false);
        map.insert(sym, value);
        let span = l.tpd.span(p);
        let at = span.end_pos();
        let qual = l.ident(all_args, at);
        let fun = l.select(qual, apply, at);
        let index = l.int_literal(i as i32);
        l.tpd.get_mut(index).span = at;
        let element = l.apply(fun, &[index], object, at);
        let Tree::ValDef { tpt, .. } = *l.tpd.tree(p) else { unreachable!("a parameter") };
        let name = l.build().term_name(value);
        defs.push(l.tpd.node(Tree::ValDef { name, tpt, rhs: Some(element) }, span, Some(info), Some(value)));
    }
    substitute(l, rhs, &map);
    let span = defs.iter().fold(l.tpd.span(rhs), |s, &d| s.union(l.tpd.span(d)));
    Ok(l.block(&defs, rhs, super::adapt::from_start(span)))
}
