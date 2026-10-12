//! Matches `sel match { cases }` and their cases (dotty's `typedMatch`, `typedCase`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::tree::MatchKind;
use syntax::ast::{Attrs, List, NodeId, Span, TName, Tree};
use tasty::{NOTHING, Type, TypeId};

impl Typer<'_> {
    pub fn typed_match(
        &mut self,
        id: NodeId,
        selector: Option<NodeId>,
        cases: List,
        kind: MatchKind,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let Some(selector) = selector else {
            if kind == MatchKind::Inline {
                return self.unsupported(id, "summonFrom");
            }
            return self.typed_case_lambda(id, cases, pt, ctx);
        };
        if kind == MatchKind::Inline {
            self.check_in_inline_context("inline match", self.ast.span(id), ctx);
        }
        let sel = self.typed(selector, Pt::Any, ctx);
        if self.tpd.sym(sel).is_some_and(|s| self.syms.repeated_params.contains_key(&s)) {
            return self.unsupported(id, "match on a repeated parameter");
        }
        let sel_tpe = self.tpd.tpe(sel);
        let sel_tpe = match self.fully_defined_as(sel_tpe, true) {
            Ok(t) => t,
            Err(what) => return self.unsupported(id, what),
        };
        let constant = matches!(self.u.types().get(sel_tpe), Type::Constant(..) | Type::Str(_));
        let sel_tpe = if !constant && self.is_singleton(sel_tpe) { self.u.widen(sel_tpe) } else { sel_tpe };
        let case_pt = match pt {
            Pt::Fun(_) | Pt::PolyFun => Pt::Any,
            pt => pt,
        };
        let case_ids = self.ast.items(cases).to_vec();
        let typed_cases: Vec<TNodeId> =
            case_ids.iter().map(|&c| self.typed_case(c, sel_tpe, case_pt, ctx)).collect();
        if typed_cases.is_empty() {
            return self.unsupported(id, "match without cases");
        }
        let mut bodies: Vec<TypeId> = Vec::new();
        for &c in &typed_cases {
            let T::CaseDef { body, .. } = *self.tpd.tree(c) else { return self.unsupported(id, "case") };
            bodies.push(self.tpd.tpe(body));
        }
        if matches!(pt, Pt::Any) && bodies.windows(2).any(|w| self.needs_harmonizing(w[0], w[1])) {
            return self.unsupported(id, "numeric harmonization");
        }
        let tpe = bodies.iter().fold(NOTHING, |acc, &b| self.u.lub(acc, b));
        let attrs = self.ast.attrs(selector);
        let sel = if attrs.contains(Attrs::CHECK_PAT_DEF) || attrs.contains(Attrs::CHECK_GEN_FROM) {
            let T::CaseDef { pat, .. } = *self.tpd.tree(typed_cases[0]) else { unreachable!("a case") };
            match self.runtime_checked(sel, pat, sel_tpe) {
                Ok(sel) => sel,
                Err(what) => return self.unsupported(id, what),
            }
        } else {
            sel
        };
        let cases = self.tpd.list(&typed_cases);
        let span = self.ast.span(id);
        self.tpd.node(T::Match { selector: sel, cases, kind }, span, Some(tpe), None)
    }

    /// `{ cases }` as `x$1 => x$1 match { cases }` (dotty's `makeCaseLambda`).
    fn typed_case_lambda(&mut self, id: NodeId, cases: List, pt: Pt, ctx: Ctx) -> TNodeId {
        let items = self.ast.items(cases).to_vec();
        let (Some(&first), Some(&last)) = (items.first(), items.last()) else {
            return self.unsupported(id, "case lambda without cases");
        };
        let (start, end) = (self.ast.span(first).start(), self.ast.span(last).end());
        let at = Span::synthetic(start, start);
        let name = TName { name: self.name("x$1"), is_type: false };
        let tpt = self.ast.node(Tree::TypeTree);
        let mods = self.ast.mods(Modifiers::with_flags(flags::SYNTHETIC_TERM_PARAM));
        let param = self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods });
        self.ast.with_span(param, at);
        let selector = self.ast.node(Tree::Ident { name });
        self.ast.with_span(selector, at);
        let body = self.ast.node(Tree::Match { selector: Some(selector), cases, kind: MatchKind::Plain });
        self.ast.with_span(body, Span::synthetic(start, end));
        let args = self.ast.list(&[param]);
        self.typed_function(id, args, body, pt, ctx)
    }

    /// Selector `sel` of type `sel_tpe` of a pattern definition or generator with pattern `pat`,
    /// ascribed `@RuntimeChecked`; `Err` unless `pat` is irrefutable (dotty's `checkIrrefutable`).
    fn runtime_checked(
        &mut self,
        sel: TNodeId,
        pat: TNodeId,
        sel_tpe: TypeId,
    ) -> Result<TNodeId, &'static str> {
        if matches!(self.u.types().get(sel_tpe), Type::Annotated(..)) {
            return Err("annotated selector of a pattern definition");
        }
        if !self.is_irrefutable(pat, Some(sel_tpe)) {
            return Err("refutable pattern definition");
        }
        let annot = self.resolve_full_name("scala.annotation.internal.RuntimeChecked", true);
        let annot = annot.ok_or("RuntimeChecked missing from the library")?;
        let annot = crate::defn::type_ref(self.u, annot);
        let widened = self.u.widen(sel_tpe);
        let tpe = self.u.intern_type(Type::Annotated(widened, annot));
        let span = self.tpd.span(sel);
        let tpt = self.type_tree(tpe, span.end_pos(), true);
        Ok(self.tpd.node(T::Typed { expr: sel, tpt }, span.to_synthetic(), Some(tpe), None))
    }

    /// Whether typed pattern `pat` matches every value of type `sel`: a variable or wildcard, a
    /// type pattern `sel` conforms to, or a case class extractor of irrefutable patterns; type
    /// patterns only where `sel` is known.
    fn is_irrefutable(&mut self, pat: TNodeId, sel: Option<TypeId>) -> bool {
        match *self.tpd.tree(pat) {
            T::Bind { body, .. } => self.is_irrefutable(body, sel),
            T::Ident { name } => self.text(name.name) == "_",
            T::Typed { expr, tpt } => {
                let tpe = self.tpd.tpe(tpt);
                sel.is_some_and(|s| self.u.is_subtype(s, tpe)) && self.is_irrefutable(expr, Some(tpe))
            }
            T::UnApply { fun, patterns, .. } => {
                let mut callee = fun;
                while let T::TypeApply { fun, .. } = *self.tpd.tree(callee) {
                    callee = fun;
                }
                let synthetic =
                    self.tpd.sym(callee).is_some_and(|s| self.u.symbol(s).flags.has(tasty::tags::SYNTHETIC));
                let pats = self.tpd.items(patterns).to_vec();
                synthetic && pats.iter().all(|&p| self.is_irrefutable(p, None))
            }
            _ => false,
        }
    }

    /// Case `id` of a match on a value of type `selector`, its body expected to be `pt`; a body
    /// whose type refers to the pattern's variables ascribed a type that does not.
    pub fn typed_case(&mut self, id: NodeId, selector: TypeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let Tree::CaseDef { pat, guard, body } = *self.ast.tree(id) else {
            return self.unsupported(id, "case");
        };
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        let cctx = Ctx { level, ..ctx };
        let pat = self.typed(pat, Pt::Type(selector), cctx.with_mode(ctx.mode.with(Mode::PATTERN)));
        let boolean = self.boolean();
        let guard = guard.map(|g| self.typed(g, Pt::Type(boolean), cctx));
        let typed_body = self.typed(body, pt, cctx);
        let locals = self.levels.scope_symbols(level);
        let Some(body) = self.no_local_refs(typed_body, &locals, pt) else {
            return self.unsupported(id, "case type with pattern variables");
        };
        let span = self.ast.span(id);
        self.tpd.node(T::CaseDef { pat, guard, body }, span, None, None)
    }

    /// Pattern `p1 | p2 | ...` against a value of type `pt` (dotty's `typedAlternative`).
    pub fn typed_alternative(&mut self, id: NodeId, trees: List, pt: Pt, ctx: Ctx) -> TNodeId {
        let ids = self.ast.items(trees).to_vec();
        let typed: Vec<TNodeId> = ids.iter().map(|&t| self.typed(t, pt, ctx)).collect();
        if typed.iter().any(|&t| self.binds_variable(t)) {
            return self.unsupported(id, "variable in an alternative");
        }
        let tpe = typed.iter().fold(NOTHING, |acc, &t| {
            let t = self.tpd.tpe(t);
            self.u.lub(acc, t)
        });
        let trees = self.tpd.list(&typed);
        let span = self.ast.span(id);
        self.tpd.node(T::Alternative { trees }, span, Some(tpe), None)
    }

    /// Whether pattern `t` or a pattern inside it binds a variable.
    fn binds_variable(&self, t: TNodeId) -> bool {
        matches!(self.tpd.tree(t), T::Bind { .. })
            || self.tpd.children(t).into_iter().any(|c| self.binds_variable(c))
    }
}
