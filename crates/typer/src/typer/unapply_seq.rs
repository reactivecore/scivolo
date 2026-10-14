//! Patterns of sequence extractors `C(p1, ..., x*)` (dotty's `unapplySeq` in `typedUnApply` and
//! `UnapplyArgs`): those of the product selectors before a sequence of their types, the others of
//! the element type, a last `x*` of the sequence.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, Type, TypeId};

/// The types of the patterns of a sequence extractor, and how many of them are product selectors.
pub type SeqArgs = (Vec<TypeId>, usize);

impl Typer<'_> {
    /// Whether pattern `a` is a sequence wildcard `x*` or `_*`.
    pub fn is_seq_wildcard(&self, a: NodeId) -> bool {
        let Tree::Typed { tpt, .. } = *self.ast.tree(a) else { return false };
        matches!(*self.ast.tree(tpt), Tree::Ident { name } if name.is_type && self.text(name.name) == "_*")
    }

    /// The expected types of `n` patterns of a sequence extractor with result type `result`
    /// (dotty's `UnapplyArgs.argTypes` for `unapplySeq`), and the number of product selectors
    /// before the sequence.
    pub fn unapply_seq_arg_types(&mut self, result: TypeId, n: usize) -> Result<SeqArgs, &'static str> {
        if let Some(args) = self.seq_match(result, n)? {
            return Ok(args);
        }
        let get = self.get_match(result)?.ok_or("sequence extractor result type")?;
        self.seq_match(get, n)?.ok_or("sequence extractor result type")
    }

    /// The types of `n` patterns against `t` as a sequence, or as a product whose last selector is
    /// a sequence (dotty's `unapplySeq` of `UnapplyArgs`).
    fn seq_match(&mut self, t: TypeId, n: usize) -> Result<Option<SeqArgs>, &'static str> {
        if let Some(elem) = self.seq_elem_type(t) {
            return Ok(Some((vec![elem; n], 0)));
        }
        if !self.derives_from_named(t, "scala.Product") {
            return Ok(None);
        }
        let selectors = self.product_selectors(t)?;
        let Some((&last, leading)) = selectors.split_last() else { return Ok(None) };
        let Some(elem) = self.seq_elem_type(last).filter(|_| selectors.len() <= n + 1) else {
            return Ok(None);
        };
        Ok(Some(((0..n).map(|i| leading.get(i).copied().unwrap_or(elem)).collect(), leading.len())))
    }

    /// The element type of sequence-like `t`, the result of its `apply(Int)`, if it also has
    /// `lengthCompare(Int): Int` or `length: Int`, and `drop(Int)` and `toSeq` that are
    /// `collection.Seq`s of it (dotty's `unapplySeqTypeElemTp`).
    pub fn seq_elem_type(&mut self, t: TypeId) -> Option<TypeId> {
        let elem = self.int_method_results(t, "apply").into_iter().next()?;
        let seq_class = self.resolve_full_name("scala.collection.Seq", true)?;
        let seq_class = crate::defn::type_ref(self.u, seq_class);
        let seq = self.u.intern_type(Type::Applied(seq_class, vec![elem]));
        let length = self.int_method_results(t, "lengthCompare").into_iter().any(|r| self.is_int(r))
            || self.parameterless_results(t, "length").into_iter().any(|r| self.is_int(r));
        let drop = self.int_method_results(t, "drop").into_iter().any(|r| self.u.is_subtype(r, seq));
        let to_seq = self.parameterless_results(t, "toSeq").into_iter().any(|r| self.u.is_subtype(r, seq));
        (length && drop && to_seq).then_some(elem)
    }

    /// The results of the alternatives of method `name` of `t` that take one `Int`.
    fn int_method_results(&mut self, t: TypeId, name: &str) -> Vec<TypeId> {
        let mut out = Vec::new();
        for s in self.members_found(t, name, false).map(|f| f.syms).unwrap_or_default() {
            let tpe = self.ref_type(Some(t), s);
            let Some(info) = self.method_info(tpe) else { continue };
            let Type::Method { params, result, .. } = self.u.types().get(info).clone() else { continue };
            if let [(_, index)] = params[..]
                && self.is_int(index)
                && !matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. })
            {
                out.push(result);
            }
        }
        out
    }

    fn is_int(&mut self, t: TypeId) -> bool {
        self.u.class_symbol(t).is_some_and(|c| self.u.full_name(c) == "scala.Int")
    }

    /// The results of the alternatives of member `name` of `t` that are parameterless methods.
    fn parameterless_results(&mut self, t: TypeId, name: &str) -> Vec<TypeId> {
        let mut out = Vec::new();
        for s in self.members_found(t, name, false).map(|f| f.syms).unwrap_or_default() {
            if self.u.symbol(s).kind != Kind::Def {
                continue;
            }
            let tpe = self.ref_type(Some(t), s);
            let info = self.method_info(tpe).unwrap_or(tpe);
            match *self.u.types().get(info) {
                Type::Method { .. } | Type::Poly { .. } => {}
                Type::ByName(r) => out.push(r),
                _ => out.push(info),
            }
        }
        out
    }

    /// Patterns `args` of a sequence extractor of types `types`, the first `leading` of them
    /// product selectors, the others elements.
    pub fn typed_seq_patterns(
        &mut self,
        args: &[NodeId],
        (types, leading): SeqArgs,
        ctx: Ctx,
    ) -> Result<Vec<TNodeId>, &'static str> {
        let mut out = Vec::new();
        for (i, (&a, t)) in args.iter().zip(types).enumerate() {
            if i >= leading && matches!(self.u.types().get(t), Type::Bounds(..)) {
                return Err("sequence extractor of a wildcard element type");
            }
            out.push(if !self.is_seq_wildcard(a) {
                self.typed(a, Pt::Type(t), ctx)
            } else if i + 1 == args.len() && i >= leading {
                self.seq_wildcard(a, t, ctx)?
            } else {
                return Err("sequence wildcard before the last pattern");
            });
        }
        Ok(out)
    }

    /// Sequence wildcard `a`, `x*` or `_*`, of elements of type `elem`: `x @ (_*: <repeated>[elem])`.
    fn seq_wildcard(&mut self, a: NodeId, elem: TypeId, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let Tree::Typed { expr, tpt } = *self.ast.tree(a) else { return Err("sequence pattern") };
        let Tree::Ident { name } = *self.ast.tree(expr) else { return Err("sequence pattern") };
        let seq_class =
            self.resolve_full_name("scala.collection.immutable.Seq", true).ok_or("sequence pattern")?;
        let seq_class = crate::defn::type_ref(self.u, seq_class);
        let seq = self.u.intern_type(Type::Applied(seq_class, vec![elem]));
        let repeated = self.repeated_type(elem);
        let text = self.text(name.name).to_owned();
        let star_span = self.ast.span(tpt);
        let anonymous = text == "_";
        let (ident_span, typed_span, tpt_at) = if anonymous {
            let expr_span = self.ast.span(expr);
            (expr_span, self.ast.span(a), expr_span.end_pos())
        } else {
            let at = Span::at(star_span.start());
            (at, Span::synthetic(star_span.start(), star_span.end()), at)
        };
        let star = TName { name: self.name(if anonymous { "_" } else { "_*" }), is_type: false };
        let ident = self.tpd.node(T::Ident { name: star }, ident_span, Some(seq), None);
        let tree_tpt = self.type_tree(repeated, tpt_at, false);
        let typed = self.tpd.node(T::Typed { expr: ident, tpt: tree_tpt }, typed_span, Some(repeated), None);
        if anonymous {
            return Ok(typed);
        }
        let flags = syntax::ast::flags::CASE.to_term_flags();
        let sym = self.syms.define(self.u, &text, Kind::Val, ctx.owner, flags);
        self.u.set_info(sym, seq);
        self.levels.enter(ctx.level, &text, sym);
        let tpe = self.u.intern_type(Type::Sym(None, sym));
        Ok(self.tpd.node(T::Bind { name, body: typed }, self.ast.span(a), Some(tpe), Some(sym)))
    }
}
