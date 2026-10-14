//! Matches `sel match { cases }` and their cases (dotty's `typedMatch`, `typedCase`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::tree::MatchKind;
use syntax::ast::{Attrs, List, NodeId, Tree};
use tasty::{NOTHING, SymbolId, Type, TypeId};

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
                return self.typed_summon_from(id, cases, pt, ctx);
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
        let sel_tpe = if constant { sel_tpe } else { self.widened_selector(sel_tpe) };
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
        let pat_def = attrs.contains(Attrs::CHECK_PAT_DEF);
        let sel = if pat_def || attrs.contains(Attrs::CHECK_GEN_FROM) {
            let T::CaseDef { pat, .. } = *self.tpd.tree(typed_cases[0]) else { unreachable!("a case") };
            match self.runtime_checked(sel, pat, pat_def) {
                Ok(sel) => sel,
                Err(what) => return self.unsupported(id, what),
            }
        } else {
            sel
        };
        let cases = self.tpd.list(&typed_cases);
        let span = self.ast.span(id);
        self.tpd.node(T::Match { selector: Some(sel), cases, kind }, span, Some(tpe), None)
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
        let mut locals = self.levels.scope_symbols(level);
        let mut types = Vec::new();
        self.pattern_types(pat, &mut types);
        locals.extend(types.iter().filter(|t| !locals.contains(t)).copied().collect::<Vec<_>>());
        let unconstrained = self.is_top(selector);
        let (bounded, opaque): (&[_], &[_]) = if unconstrained { (&types, &[]) } else { (&[], &types) };
        let Some(body) = self.no_local_refs(typed_body, &locals, [bounded, opaque], pt) else {
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

    /// The types bound in pattern `t`, appended to `out`.
    fn pattern_types(&self, t: TNodeId, out: &mut Vec<SymbolId>) {
        if let T::Bind { .. } = self.tpd.tree(t)
            && let Some(s) = self.tpd.sym(t)
            && self.u.is_type_symbol(s)
        {
            out.push(s);
        }
        for c in self.tpd.children(t) {
            self.pattern_types(c, out);
        }
    }

    /// Whether `t` is `Any`, `Matchable` or `AnyRef`, against which a pattern's type constrains
    /// none of its bound types.
    fn is_top(&mut self, t: TypeId) -> bool {
        let t = self.u.dealias(t);
        let b = self.u.builtins();
        let tops = [b.any, b.matchable, b.object];
        matches!(*self.u.types().get(t), Type::Sym(_, s) if tops.contains(&s))
    }

    /// Whether pattern `t` or a pattern inside it binds a variable.
    fn binds_variable(&self, t: TNodeId) -> bool {
        matches!(self.tpd.tree(t), T::Bind { .. })
            || self.tpd.children(t).into_iter().any(|c| self.binds_variable(c))
    }
}
