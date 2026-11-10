// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Match type trees (dotty's `typedMatchTypeTree` and `typedTypeCase`): a case is a
//! `MatchCase` under a type lambda over the type variables its pattern binds.

use super::Typer;
use crate::ctx::{Ctx, Level, LevelId, Mode};
use crate::tpd::TNodeId;
use syntax::ast::flags::CASE;
use syntax::ast::{List, NodeId, Tree};
use tasty::{ANY, Kind, NOTHING, NameId, SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_match_type_tree(
        &mut self,
        id: NodeId,
        (bound, selector, cases): (Option<NodeId>, NodeId, List),
        ctx: Ctx,
    ) -> TNodeId {
        let bound = bound.map(|b| self.typed_type(b, ctx)).map(|b| self.tpd.tpe(b));
        bound.inspect(|&b| self.complete_type_members(b));
        let selector = self.typed_type(selector, ctx);
        let scrutinee = self.tpd.tpe(selector);
        if self.type_arity(scrutinee) > 0 {
            return self.unsupported(id, "higher-kinded match type scrutinee");
        }
        let mut case_types = Vec::new();
        let mut lub = NOTHING;
        for c in self.ast.items(cases).to_vec() {
            let Tree::CaseDef { pat, guard: None, body } = *self.ast.tree(c) else {
                return self.unsupported(c, "match type case");
            };
            let Some((case, body, captures)) = self.typed_type_case(pat, (body, bound), ctx) else {
                return self.unsupported(c, "match type case");
            };
            if !self.u.is_reducible_case(case) {
                return self.unsupported(c, "match type pattern outside the subset");
            }
            if bound.is_some_and(|b| !self.u.is_subtype(body, b)) {
                return self.unsupported(c, "match type case body outside the bound");
            }
            if lub != ANY {
                let avoided = self.types().avoid_captures(body, &captures, 1);
                lub = if self.is_provisional_type(body) { ANY } else { self.u.lub(lub, avoided) };
            }
            case_types.push(case);
        }
        let bound = bound.unwrap_or(lub);
        let tpe = self.u.intern_match(bound, scrutinee, &case_types);
        let tpe = self.u.try_normalize(tpe).unwrap_or(tpe);
        self.type_tree(tpe, self.ast.span(id), false)
    }

    /// The type of case `pat => body`, its body's type and the type variables `pat` binds, a
    /// match type body without a bound bounded by `outer`; `None` if the case does not type.
    fn typed_type_case(
        &mut self,
        pat: NodeId,
        (body, outer): (NodeId, Option<TypeId>),
        ctx: Ctx,
    ) -> Option<(TypeId, TypeId, Vec<SymbolId>)> {
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        let cctx = Ctx { level, ..ctx };
        let first = self.u.next_symbol();
        for v in self.type_variables(pat) {
            self.capture_symbol(&v, ctx, Some(level));
        }
        let pat = match *self.ast.tree(pat) {
            Tree::Ident { name } if self.text(name.name) == "_" => {
                let name = self.fresh.next(self.names, "_$");
                let text = self.text(name).to_owned();
                let sym = self.capture_symbol(&text, ctx, None);
                self.u.intern_type(Type::Sym(None, sym))
            }
            _ => {
                let pat = self.typed_type(pat, cctx.with_mode(ctx.mode.with(Mode::PATTERN_TYPE)));
                if self.is_error(pat) {
                    return None;
                }
                self.tpd.tpe(pat)
            }
        };
        let mut captures: Vec<SymbolId> = Vec::new();
        self.u.map_type(pat, &mut |u, part| {
            if let Some(s) = u.sym_of(part)
                && s.0 >= first.0
                && u.symbol(s).kind == Kind::Type
                && u.symbol(s).tree.is_none()
                && !captures.contains(&s)
            {
                captures.push(s);
            }
            None
        });
        self.bound_captures(pat, &captures);
        let unbounded = matches!(self.ast.tree(body), Tree::MatchTypeTree { bound: None, .. });
        let body = self.typed_type(body, cctx);
        if self.is_error(body) {
            return None;
        }
        let mut body = self.tpd.tpe(body);
        if let Some(bound) = outer.filter(|_| unbounded)
            && let Type::Match { scrutinee, cases, .. } = self.u.ty(body).clone()
        {
            body = self.u.intern_type(Type::Match { bound, scrutinee, cases });
        }
        let case = self.u.intern_type(Type::MatchCase(pat, body));
        if captures.is_empty() {
            return Some((case, body, captures));
        }
        let binder = self.syms.fresh_binder();
        let params: Vec<(NameId, TypeId)> =
            captures.iter().map(|&s| (self.u.interned_name(s), self.u.info(s))).collect();
        let result = self.u.map_type(case, &mut |u, t| {
            let i = captures.iter().position(|&c| Some(c) == u.sym_of(t))?;
            Some(u.intern_type(Type::Param(binder, i as u32)))
        });
        Some((self.u.intern_lambda(binder, &params, result), body, captures))
    }

    /// A type variable `name` of a match type case, unbounded, entered in `level` if given.
    fn capture_symbol(&mut self, name: &str, ctx: Ctx, level: Option<LevelId>) -> SymbolId {
        let sym = self.syms.define(self.u, name, Kind::Type, ctx.owner, CASE);
        let bounds = self.u.intern_type(Type::Bounds(NOTHING, ANY));
        self.u.set_info(sym, bounds);
        if let Some(level) = level {
            self.levels.enter(level, name, sym);
        }
        sym
    }

    /// The names of the type variables pattern `pat` binds: its lower-case type identifiers; a
    /// repeated one is reported.
    fn type_variables(&mut self, pat: NodeId) -> Vec<String> {
        let idents =
            self.ast.subtrees(pat, &|a, n| matches!(a.tree(n), Tree::Ident { name } if name.is_type));
        let mut names = Vec::new();
        for i in idents {
            let Tree::Ident { name } = *self.ast.tree(i) else { continue };
            let text = self.text(name.name).to_owned();
            if !text.starts_with(|c: char| c.is_lowercase()) {
                continue;
            }
            if names.contains(&text) {
                self.error_at(self.ast.span(i), super::errors::duplicate_bind(&text));
            } else {
                names.push(text);
            }
        }
        names
    }
}
