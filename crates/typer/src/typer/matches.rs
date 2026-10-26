// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
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
            Pt::Fun(_) | Pt::PolyFun(_) => Pt::Any,
            pt => pt,
        };
        let case_ids = self.ast.items(cases).to_vec();
        let ctx = match kind {
            MatchKind::Inline => ctx.with_mode(ctx.mode.with(Mode::INLINE_CASE)),
            _ => ctx,
        };
        let dependent = self.dependent_match_cases(pt, sel, &case_ids);
        let typed_cases: Vec<TNodeId> = match &dependent {
            Some(case_types) => {
                let pairs: Vec<(NodeId, TypeId)> = case_ids.iter().copied().zip(case_types.clone()).collect();
                pairs
                    .into_iter()
                    .map(|(c, ct)| self.typed_case_in(c, sel_tpe, case_pt, Some(ct), ctx))
                    .collect()
            }
            None => {
                let before = self.before_harmonic();
                let typed: Vec<TNodeId> =
                    case_ids.iter().map(|&c| self.typed_case(c, sel_tpe, case_pt, ctx)).collect();
                if typed.is_empty() {
                    return self.unsupported(id, "match without cases");
                }
                match self.harmonic(typed, case_pt, Some(before), ctx) {
                    Ok(h) => h,
                    Err(what) => return self.unsupported(id, what),
                }
            }
        };
        let mut bodies: Vec<TypeId> = Vec::new();
        for &c in &typed_cases {
            let T::CaseDef { body, .. } = *self.tpd.tree(c) else { return self.unsupported(id, "case") };
            bodies.push(self.tpd.tpe(body));
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
        let m = self.tpd.node(T::Match { selector: Some(sel), cases, kind }, span, Some(tpe), None);
        match (dependent, pt) {
            (Some(_), Pt::Type(pt)) => self.type_cast(m, pt),
            _ => m,
        }
    }

    /// Case `id` of a match on a value of type `selector`, its body expected to be `pt`; a body
    /// whose type refers to the pattern's variables ascribed a type that does not.
    pub fn typed_case(&mut self, id: NodeId, selector: TypeId, pt: Pt, ctx: Ctx) -> TNodeId {
        self.typed_case_in(id, selector, pt, None, ctx)
    }

    /// `typed_case`, the body expected to be the body of match type case `case` if given.
    fn typed_case_in(
        &mut self,
        id: NodeId,
        selector: TypeId,
        pt: Pt,
        case: Option<TypeId>,
        ctx: Ctx,
    ) -> TNodeId {
        let Tree::CaseDef { pat, guard, body } = *self.ast.tree(id) else {
            return self.unsupported(id, "case");
        };
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        let cctx = Ctx { level, mode: ctx.mode.without(Mode::INLINE_CASE), ..ctx };
        let pat = self.typed(pat, Pt::Type(selector), cctx.with_mode(ctx.mode.with(Mode::PATTERN)));
        let body_pt = case.map(|c| self.dependent_case_body_pt(pat, c));
        let pt = match body_pt {
            Some(Some(b)) => Pt::Type(b),
            Some(None) => return self.unsupported(id, "case pattern other than its match type's"),
            None => pt,
        };
        let boolean = self.boolean();
        let guard = guard.map(|g| self.typed(g, Pt::Type(boolean), cctx));
        let typed_body = self.typed(body, pt, cctx);
        let mut locals = self.levels.scope_symbols(level);
        let mut types = Vec::new();
        self.pattern_types(pat, &mut types, false);
        locals.extend(types.iter().map(|&(t, _)| t).filter(|t| !locals.contains(t)).collect::<Vec<_>>());
        let opaque: Vec<SymbolId> = match self.is_top(selector) {
            true => Vec::new(),
            false => types.iter().filter(|&&(_, known)| !known).map(|&(t, _)| t).collect(),
        };
        let body = self.no_local_refs(typed_body, &locals, &opaque, pt);
        let body = match (body, pt) {
            (None, Pt::Type(p)) if case.is_some() => self.dependent_case_body(typed_body, p),
            (body, _) => body,
        };
        let Some(body) = body else {
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
