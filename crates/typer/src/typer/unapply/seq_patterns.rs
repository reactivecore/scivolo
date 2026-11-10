// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Patterns of sequence extractors `C(p1, ..., x*)` (dotty's `unapplySeq` in `typedUnApply`):
//! those of the product selectors before the sequence, the others of the element type, a last
//! `x*` of the sequence.

use super::super::unapply_args::SeqArgs;
use super::super::{Pt, Typer};
use super::UnapplyPattern;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, Type, TypeId};

/// The written name of a sequence wildcard's type, `x: _*`.
const SEQ_WILDCARD: &str = "_*";

impl UnapplyPattern {
    /// Types the argument patterns of a sequence extractor against `types`, the first `leading`
    /// of them product selectors, the others elements. Refuses a wildcard element type and a
    /// sequence wildcard `xs*` other than one last among the elements.
    pub fn typed_seq_patterns(
        &self,
        t: &mut Typer<'_>,
        SeqArgs { types, leading }: SeqArgs,
    ) -> Result<Vec<TNodeId>, &'static str> {
        let mut out = Vec::new();
        let mut wildcards = 0;
        for (i, (&a, tpe)) in self.args.iter().zip(types).enumerate() {
            if i >= leading && matches!(t.u.types().get(tpe), Type::Bounds(..)) {
                return Err("sequence extractor of a wildcard element type");
            }
            out.push(if !is_seq_wildcard(t, a) {
                t.typed(a, Pt::Type(tpe), self.ctx)
            } else if i >= leading && wildcards == 0 {
                wildcards += 1;
                self.seq_wildcard(t, a, tpe)?
            } else {
                return Err("sequence wildcard before the sequence or after another");
            });
        }
        Ok(out)
    }

    /// Sequence wildcard `a`, `x*` or `_*`, of elements of type `elem`: `x @ (_*: <repeated>[elem])`.
    fn seq_wildcard(&self, t: &mut Typer<'_>, a: NodeId, elem: TypeId) -> Result<TNodeId, &'static str> {
        let Tree::Typed { expr, tpt } = *t.ast.tree(a) else { return Err("sequence pattern") };
        let Tree::Ident { name } = *t.ast.tree(expr) else { return Err("sequence pattern") };
        let seq_class =
            t.resolve_full_name("scala.collection.immutable.Seq", true).ok_or("sequence pattern")?;
        let seq_class = crate::defn::type_ref(t.u, seq_class);
        let seq = t.u.intern_type(Type::Applied(seq_class, vec![elem]));
        let repeated = t.repeated_type(elem);

        let text = t.text(name.name).to_owned();
        let star_span = t.ast.span(tpt);
        let anonymous = text == "_";
        let (ident_span, typed_span, tpt_at) = if anonymous {
            let expr_span = t.ast.span(expr);
            (expr_span, t.ast.span(a), expr_span.end_pos())
        } else {
            let at = Span::at(star_span.start());
            (at, Span::synthetic(star_span.start(), star_span.end()), at)
        };
        let star = TName { name: t.name(if anonymous { "_" } else { SEQ_WILDCARD }), is_type: false };
        let ident = t.tpd.node(T::Ident { name: star }, ident_span, Some(seq), None);
        let tree_tpt = t.type_tree(repeated, tpt_at, false);
        let typed = t.tpd.node(T::Typed { expr: ident, tpt: tree_tpt }, typed_span, Some(repeated), None);
        if anonymous {
            return Ok(typed);
        }

        let flags = syntax::ast::flags::CASE.to_term_flags();
        let sym = t.syms.define(t.u, &text, Kind::Val, self.ctx.owner, flags);
        t.u.set_info(sym, seq);
        t.levels.enter(self.ctx.level, &text, sym);
        let tpe = t.u.intern_type(Type::Sym(None, sym));
        Ok(t.tpd.node(T::Bind { name, body: typed }, t.ast.span(a), Some(tpe), Some(sym)))
    }
}

/// Whether pattern `a` is a sequence wildcard `x*` or `_*`.
pub fn is_seq_wildcard(t: &Typer<'_>, a: NodeId) -> bool {
    let Tree::Typed { tpt, .. } = *t.ast.tree(a) else { return false };
    matches!(*t.ast.tree(tpt), Tree::Ident { name } if name.is_type && t.text(name.name) == SEQ_WILDCARD)
}
