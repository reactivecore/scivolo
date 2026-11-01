// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/QuotesAndSplices.scala
//! Splices in quote patterns (dotty's `typedSplicePattern` and `typedAppliedSplice`): `$x`,
//! the pattern of the matched expression, and `$f(args)`, a higher-order pattern.

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Tree};
use tasty::TypeId;

impl Typer<'_> {
    /// Splice `$body` at `id` applied to `args` (`$body(args)`) in a quote pattern, matching an
    /// expression of type `pt`.
    pub fn typed_splice_pattern(
        &mut self,
        id: NodeId,
        body: NodeId,
        args: &[NodeId],
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let Some(outer) = self.quote_pattern.as_ref().map(|q| q.outer) else {
            return self.unsupported(id, "splice pattern outside a quote pattern");
        };
        let pt = match pt {
            Pt::Type(p) if self.is_fully_defined(p) => p,
            Pt::Type(p) => match self.fully_defined_as(p, true) {
                Ok(p) => p,
                Err(what) => return self.unsupported(id, what),
            },
            _ => return self.unsupported(id, "splice pattern of a type not fully defined"),
        };
        let mut typed_args = Vec::new();
        for &a in args {
            if !matches!(self.ast.tree(a), Tree::Ident { .. }) {
                return self.unsupported(a, "open pattern argument other than an identifier");
            }
            typed_args.push(self.typed(a, Pt::Any, ctx));
        }
        let mut parts: Vec<TypeId> =
            typed_args.iter().map(|&a| self.tpd.tpe(a)).map(|t| self.u.widen(t)).collect();
        let pat_type = match parts.is_empty() {
            true => pt,
            false => {
                parts.push(pt);
                match self.function_of(parts) {
                    Ok(f) => f,
                    Err(what) => return self.unsupported(id, what),
                }
            }
        };
        let Some(expr) = self.resolve_full_name("scala.quoted.Expr", true) else {
            return self.unsupported(id, "splice pattern without scala.quoted");
        };
        let expr_pt = crate::defn::applied(self.u, expr, vec![pat_type]);
        let expr_pt = self.simplified(expr_pt);
        let mode = outer.mode.without(Mode::QUOTED_PATTERN).with(Mode::PATTERN);
        let pctx = Ctx { mode, staging: ctx.staging - 1, ..outer };
        let body = self.typed(body, Pt::Type(expr_pt), pctx);
        let typeargs = self.tpd.list(&[]);
        let args = self.tpd.list(&typed_args);
        self.tpd.node(T::SplicePattern { body, typeargs, args }, self.ast.span(id), Some(pt), None)
    }

    /// `$f(args)` at `id`, splice `splice` applied in a quote pattern, matching an expression of
    /// type `pt`.
    pub fn typed_applied_splice(
        &mut self,
        id: NodeId,
        splice: NodeId,
        args: List,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let Tree::SplicePattern { body } = *self.ast.tree(splice) else {
            return self.unsupported(id, "splice");
        };
        if self.ast.span(splice).end() != self.ast.span(body).end() {
            return self.unsupported(id, "applied splice in braces in a quote pattern");
        }
        let args = self.ast.items(args).to_vec();
        if args.is_empty() {
            self.error_at(self.ast.span(id), "Missing arguments for open pattern");
        }
        self.typed_splice_pattern(id, body, &args, pt, ctx)
    }
}
