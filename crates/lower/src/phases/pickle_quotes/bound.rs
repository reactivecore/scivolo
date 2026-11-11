// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/BetaReduce.scala
//! A hole's content with its parameters bound to the hole's arguments.

use crate::Lower;
use crate::phases::lazy_trees::val_def;
use syntax::ast::Span;
use tasty::{SymbolId, nme};
use typer::tpd::TNodeId;

/// `{ val p_j = contents.apply(j).asInstanceOf[C[T_j]]; content }` of the parameters `params`
/// of `content`, owned by `anon`, `contents` the hole's arguments (dotty's `BetaReduce` of the
/// content applied to them); the definitions at `at`, the end of the first content bound if
/// unset, as dotty shares the reference to `contents`.
pub fn bound(
    l: &mut Lower,
    (content, params): (TNodeId, &[SymbolId]),
    (contents, anon): (SymbolId, SymbolId),
    at: &mut Span,
) -> TNodeId {
    if params.is_empty() {
        return content;
    }
    let seq = l.u.class_ref(tasty::defn::IMMUTABLE_SEQ);
    let seq = l.u.class_symbol(seq).expect("Seq");
    let apply = l.u.member(seq, nme::APPLY)[0];
    if !at.exists() {
        *at = l.tpd.span(content).end_pos().to_synthetic();
    }
    let at = *at;
    let bindings: Vec<TNodeId> = params
        .iter()
        .enumerate()
        .map(|(j, &p)| {
            l.u.set_owner(p, anon);
            let qual = l.ident(contents, Span::NONE);
            let fun = l.select(qual, apply, Span::NONE);
            let fun = l.positioned(fun);
            let index = l.int_literal(j as i32);
            let arg = l.apply(fun, &[index], tasty::ANY, Span::NONE);
            let arg = l.positioned(arg);
            let info = l.u.info(p);
            let cast = l.ensure_conforms(arg, info);
            let def = val_def(l, p, cast);
            l.with_span(def, at)
        })
        .collect();
    let tpe = l.tpd.tpe(content);
    let block = l.block(&bindings, content, Span::NONE);
    l.tpd.get_mut(block).tpe = Some(tpe);
    l.positioned(block)
}
