// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Patterns of sequence extractors `C(p1, ..., x*)` (dotty's `unapplySeq` in `typedUnApply`):
//! those of the product selectors before the sequence, the others of the element type, a last
//! `x*` of the sequence.

use super::super::unapply_args::{SeqArgs, UnapplyArgs};
use super::super::{Pt, Typer};
use super::{SEQ_WILDCARD, UnapplyPattern, is_named_pattern, is_seq_wildcard};
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, Type, TypeId};

impl UnapplyPattern {
    /// Types the argument patterns of an `unapplySeq` with result type `result`, those before the
    /// sequence against product selectors, the others against the element type. Refuses named
    /// patterns, a wildcard element type and a sequence wildcard `xs*` other than one last among
    /// the elements.
    pub fn typed_seq_patterns(
        &self,
        t: &mut Typer<'_>,
        result: TypeId,
    ) -> Result<Vec<TNodeId>, &'static str> {
        if self.args.iter().any(|&a| is_named_pattern(t, a)) {
            return Err("named pattern of a sequence extractor");
        }
        let SeqArgs { types, leading } = UnapplyArgs { result, n: self.args.len() }.seq_arg_types(t)?;

        let mut out = Vec::new();
        let mut wildcards = 0;
        for (i, (&a, tpe)) in self.args.iter().zip(types).enumerate() {
            if i >= leading && t.u.bounds(tpe).is_some() {
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

    /// Types sequence wildcard `a`, `x*` or `_*`, of elements of type `elem` as
    /// `x @ (_*: <repeated>[elem])`.
    fn seq_wildcard(&self, t: &mut Typer<'_>, a: NodeId, elem: TypeId) -> Result<TNodeId, &'static str> {
        let Tree::Typed { expr, tpt } = *t.ast.tree(a) else { return Err("sequence pattern") };
        let Tree::Ident { name } = *t.ast.tree(expr) else { return Err("sequence pattern") };
        let seq_class = t.resolve_full_name(crate::defn::IMMUTABLE_SEQ, true).ok_or("sequence pattern")?;
        let seq_class = crate::defn::type_ref(t.u, seq_class);
        let seq = t.u.intern_applied(seq_class, &[elem]);
        let repeated = t.repeated_type(elem);

        let text = t.text(name.name).to_owned();
        let star_span = t.ast.span(tpt);
        let anonymous = text == nme::WILDCARD;
        let (ident_span, typed_span, tpt_at) = if anonymous {
            let expr_span = t.ast.span(expr);
            (expr_span, t.ast.span(a), expr_span.end_pos())
        } else {
            let at = Span::at(star_span.start());
            (at, Span::synthetic(star_span.start(), star_span.end()), at)
        };
        let star =
            TName { name: t.name(if anonymous { nme::WILDCARD } else { SEQ_WILDCARD }), is_type: false };
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
