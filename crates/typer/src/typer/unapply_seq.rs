//! Patterns of sequence extractors `C(p1, ..., x*)` (dotty's `unapplySeq` in `typedUnApply` and
//! `UnapplyArgs`): each pattern of the element type, a last `x*` of the sequence.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// Whether pattern `a` is a sequence wildcard `x*` or `_*`.
    pub fn is_seq_wildcard(&self, a: NodeId) -> bool {
        let Tree::Typed { tpt, .. } = *self.ast.tree(a) else { return false };
        matches!(*self.ast.tree(tpt), Tree::Ident { name } if name.is_type && self.text(name.name) == "_*")
    }

    /// The element type of sequence-like `t`, the result of its `apply(Int)`, if it also has
    /// `lengthCompare(Int)`, `drop(Int)` and a `toSeq` that is a `collection.Seq` (dotty's
    /// `unapplySeqTypeElemTp`).
    pub fn seq_elem_type(&mut self, t: TypeId) -> Option<TypeId> {
        let to_seq = self.extractor_member(t, "toSeq").ok()??;
        if !self.derives_from_named(to_seq, "scala.collection.Seq") {
            return None;
        }
        self.int_method_result(t, "lengthCompare")?;
        self.int_method_result(t, "drop")?;
        self.int_method_result(t, "apply")
    }

    /// The result of the alternative of method `name` of `t` that takes one `Int`.
    fn int_method_result(&mut self, t: TypeId, name: &str) -> Option<TypeId> {
        let found = self.members_found(t, name, false)?;
        for s in found.syms {
            let tpe = self.ref_type(Some(t), s);
            let Some(info) = self.method_info(tpe) else { continue };
            let Type::Method { params, result, .. } = self.u.types().get(info).clone() else { continue };
            let [(_, index)] = params[..] else { continue };
            if self.u.class_symbol(index).is_some_and(|c| self.u.full_name(c) == "scala.Int") {
                return Some(result);
            }
        }
        None
    }

    /// Patterns `args` of a sequence extractor of elements of type `elem`.
    pub fn typed_seq_patterns(
        &mut self,
        args: &[NodeId],
        elem: TypeId,
        ctx: Ctx,
    ) -> Result<Vec<TNodeId>, &'static str> {
        let mut out = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            out.push(if !self.is_seq_wildcard(a) {
                self.typed(a, Pt::Type(elem), ctx)
            } else if i + 1 == args.len() {
                self.seq_wildcard(a, elem, ctx)?
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
