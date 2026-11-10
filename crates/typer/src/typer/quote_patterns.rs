// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/QuotesAndSplices.scala
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! Quote patterns `case '{ ... }` and `case '[ ... ]` (dotty's `typedQuotePattern` and
//! `desugar.quotedPattern`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{ANY, SymbolId, TypeId};

/// The quote pattern being typed: the context of the pattern and its type variables by name.
pub struct QuotePatternScope {
    pub outer: Ctx,
    pub type_vars: Vec<(String, SymbolId)>,
    /// The last symbol defined before the pattern, where its type variables count as defined.
    pub before: SymbolId,
}

impl Typer<'_> {
    /// `'{ body }` or `'[ body ]` as a pattern against a value of type `pt`.
    pub fn typed_quote_pattern(&mut self, id: NodeId, body: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let span = self.ast.span(id);
        let is_type = self.ast.is_type(body);
        let class_name = if is_type { "scala.quoted.Type" } else { "scala.quoted.Expr" };
        let Some(class) = self.resolve_full_name(class_name, true) else {
            return self.unsupported(id, "quote pattern without scala.quoted");
        };
        let quotes = match self.quotes_arg(span, ctx) {
            Ok(q) => q,
            Err(what) => return self.unsupported(id, what),
        };
        let pt = match pt {
            Pt::Type(p) => p,
            _ => ANY,
        };
        if !self.derives_from_named(pt, class_name) {
            let msg = format!("Quote pattern can only match scrutinees of type {class_name}");
            self.error_at(span, msg);
        }
        let base = self.u.base_type(pt, class).and_then(|b| self.u.applied(b));
        let quoted_pt = match base {
            Some((_, args)) if self.u.bounds(args[0]).is_none() => args[0],
            _ => ANY,
        };
        let body = self.adapted_to_expected(body, quoted_pt);
        if self.defines_type_variables(body) {
            return self.unsupported(id, "type variable definitions in a quote pattern");
        }
        let saved = self.quote_pattern.replace(QuotePatternScope {
            outer: ctx,
            type_vars: Vec::new(),
            before: SymbolId(self.u.next_symbol().0 - 1),
        });
        let quoted = if is_type { Mode::QUOTED_TYPE_PATTERN } else { Mode::QUOTED_EXPR_PATTERN };
        let qctx =
            Ctx { mode: ctx.mode.without(Mode::PATTERN).with(quoted), staging: ctx.staging + 1, ..ctx };
        let body = if is_type { self.typed_type(body, qctx) } else { self.typed(body, Pt::Any, qctx) };
        self.quote_pattern = saved;
        let mut bindings = Vec::new();
        self.extract_type_bindings(body, 1, &mut bindings);
        self.check_quote_pattern(body);
        let tpe = self.tpd.tpe(body);
        let tpe = crate::defn::applied(self.u, class, vec![tpe]);
        let list = self.tpd.list(&bindings);
        let pattern = self.tpd.node(T::QuotePattern { bindings: list, body, quotes }, span, Some(tpe), None);
        if ctx.staging == 0 {
            self.record_pattern_givens(pattern, body, &bindings, ctx);
            if self.features.capture_checking() {
                if self.refers_to_capturing(body) {
                    return self.unsupported(id, "quote referring to capturing types under capture checking");
                }
                self.mark_inferred(body);
            }
        }
        pattern
    }

    /// The `Quotes` a quote pattern at `span` matches with.
    fn quotes_arg(&mut self, span: Span, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let class = self.resolve_full_name("scala.quoted.Quotes", true).ok_or("Quotes")?;
        let tpe = crate::defn::type_ref(self.u, class);
        let at = Span::at(span.start());
        let quotes = self.implicit_arg(tpe, at, ctx.with_mode(ctx.mode.without(Mode::PATTERN)), 0)?;
        self.tpd.get_mut(quotes).span = at;
        Ok(quotes)
    }

    /// Quote pattern body `t` with each splice at a leaf ascribed `quoted_pt`, the type the
    /// scrutinee quotes (dotty's `desugar.quotedPattern`).
    fn adapted_to_expected(&mut self, t: NodeId, quoted_pt: TypeId) -> NodeId {
        let tree = self.ast.tree(t).clone();
        let leaf = |p: &mut Self, t| p.adapted_to_expected(t, quoted_pt);
        let adapted = match tree {
            Tree::SplicePattern { .. } => {
                let span = self.ast.span(t);
                let tpt = self.expected_tpt(quoted_pt, span.end());
                let typed = self.ast.node(Tree::Typed { expr: t, tpt });
                self.ast.with_span(typed, span);
                return typed;
            }
            Tree::Typed { expr, tpt } if matches!(self.ast.tree(expr), Tree::SplicePattern { .. }) => {
                let (outer, span) = (self.ast.span(t), self.ast.span(tpt));
                let parens = outer.end().saturating_sub(span.end());
                let span = match parens {
                    0 => span,
                    _ => Span::new(span.start() - parens, outer.end(), span.start() - parens),
                };
                let expected = self.expected_tpt(quoted_pt, span.end());
                let and = TName { name: self.name("scala.&"), is_type: true };
                let and = self.ast.node(Tree::TypedSplice { sym: and });
                self.ast.with_span(and, Span::at(span.start()));
                let args = self.ast.list(&[tpt, expected]);
                let tpt = self.ast.node(Tree::AppliedTypeTree { tpt: and, args });
                self.ast.with_span(tpt, span);
                Tree::Typed { expr, tpt }
            }
            Tree::Block { stats, expr: Some(e) } => Tree::Block { stats, expr: Some(leaf(self, e)) },
            Tree::If { cond, thenp, elsep, inline } => {
                Tree::If { cond, thenp: leaf(self, thenp), elsep: elsep.map(|e| leaf(self, e)), inline }
            }
            Tree::Parens { t: inner } => Tree::Parens { t: leaf(self, inner) },
            Tree::Match { selector, cases, kind } => {
                let items = self.ast.items(cases).to_vec();
                let adapted: Vec<NodeId> = items
                    .into_iter()
                    .map(|c| match *self.ast.tree(c) {
                        Tree::CaseDef { pat, guard, body } => {
                            let body = leaf(self, body);
                            self.ast.derived(c, Tree::CaseDef { pat, guard, body })
                        }
                        _ => c,
                    })
                    .collect();
                Tree::Match { selector, cases: self.ast.list(&adapted), kind }
            }
            Tree::ParsedTry { expr, handler, finalizer } => {
                Tree::ParsedTry { expr: leaf(self, expr), handler: handler.map(|h| leaf(self, h)), finalizer }
            }
            _ => return t,
        };
        self.ast.derived(t, adapted)
    }

    /// An untyped tree standing for a type tree of `tpe` at `offset`.
    fn expected_tpt(&mut self, tpe: TypeId, offset: syntax::source::Offset) -> NodeId {
        let at = Span::at(offset);
        let typed = self.type_tree(tpe, at, false);
        let tpt = self.ast.node(Tree::TypeTree);
        self.ast.with_span(tpt, at);
        self.splices.insert(tpt, typed);
        tpt
    }

    /// Whether quote pattern body `t` starts with type variable definitions `type t`.
    fn defines_type_variables(&self, t: NodeId) -> bool {
        let Tree::Block { stats, .. } = *self.ast.tree(t) else { return false };
        self.ast.items(stats).first().is_some_and(|&s| match *self.ast.tree(s) {
            Tree::TypeDef { name, .. } => self.is_var_pattern(s, self.text(name.name)),
            _ => false,
        })
    }
}
