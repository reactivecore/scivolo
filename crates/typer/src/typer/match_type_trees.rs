//! Match type trees (dotty's `typedMatchTypeTree` and `typedTypeCase`): a case is a
//! `MatchCase` under a type lambda over the type variables its pattern binds.

use super::Typer;
use crate::ctx::{Ctx, Level, Mode};
use crate::tpd::TNodeId;
use syntax::ast::flags::CASE;
use syntax::ast::{List, NodeId, Tree};
use tasty::{ANY, Kind, NOTHING, SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_match_type_tree(
        &mut self,
        id: NodeId,
        (bound, selector, cases): (Option<NodeId>, NodeId, List),
        ctx: Ctx,
    ) -> TNodeId {
        let bound = bound.map(|b| self.typed_type(b, ctx));
        let selector = self.typed_type(selector, ctx);
        let scrutinee = self.tpd.tpe(selector);
        let mut case_types = Vec::new();
        let mut lub = NOTHING;
        for c in self.ast.items(cases).to_vec() {
            let Tree::CaseDef { pat, guard: None, body } = *self.ast.tree(c) else {
                return self.unsupported(c, "match type case");
            };
            let Some((case, body)) = self.typed_type_case(pat, body, ctx) else {
                return self.unsupported(c, "match type case");
            };
            lub = self.u.lub(lub, body);
            case_types.push(case);
        }
        let bound = bound.map_or(lub, |b| self.tpd.tpe(b));
        let tpe = self.u.intern_type(Type::Match { bound, scrutinee, cases: case_types });
        self.type_tree(tpe, self.ast.span(id), false)
    }

    /// The type of case `pat => body` and its body's type, the type variables of `pat` avoided
    /// as `Any`; `None` if the case does not type.
    fn typed_type_case(&mut self, pat: NodeId, body: NodeId, ctx: Ctx) -> Option<(TypeId, TypeId)> {
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        let cctx = Ctx { level, ..ctx };
        let first = self.u.next_symbol();
        for v in self.type_variables(pat) {
            let sym = self.syms.define(self.u, &v, Kind::Type, ctx.owner, CASE);
            let bounds = self.u.intern_type(Type::Bounds(NOTHING, ANY));
            self.u.set_info(sym, bounds);
            self.levels.enter(level, &v, sym);
        }
        let wildcard = matches!(*self.ast.tree(pat), Tree::Ident { name } if self.text(name.name) == "_");
        let pat = match wildcard {
            true => {
                let name = self.fresh.next(self.names, "_$");
                let text = self.text(name).to_owned();
                let sym = self.syms.define(self.u, &text, Kind::Type, ctx.owner, CASE);
                let bounds = self.u.intern_type(Type::Bounds(NOTHING, ANY));
                self.u.set_info(sym, bounds);
                self.u.intern_type(Type::Sym(None, sym))
            }
            false => {
                let pat = self.typed_type(pat, cctx.with_mode(ctx.mode.with(Mode::PATTERN_TYPE)));
                if self.is_error(pat) {
                    return None;
                }
                self.tpd.tpe(pat)
            }
        };
        let body = self.typed_type(body, cctx);
        if self.is_error(body) {
            return None;
        }
        let body = self.tpd.tpe(body);
        let mut captures: Vec<SymbolId> = Vec::new();
        self.u.map_type(pat, &mut |u, part| {
            if let Type::Sym(_, s) = *u.types().get(part)
                && s.0 >= first.0
                && u.symbol(s).kind == Kind::Type
                && !captures.contains(&s)
            {
                captures.push(s);
            }
            None
        });
        let case = self.u.intern_type(Type::MatchCase(pat, body));
        if captures.is_empty() {
            return Some((case, body));
        }
        let binder = self.syms.fresh_binder();
        let params: Vec<(String, TypeId)> =
            captures.iter().map(|&s| (self.u.symbol(s).name.clone(), self.u.info(s))).collect();
        let capture = |u: &tasty::Universe, t: TypeId| match *u.types().get(t) {
            Type::Sym(_, s) => captures.iter().position(|&c| c == s),
            _ => None,
        };
        let result = self
            .u
            .map_type(case, &mut |u, t| capture(u, t).map(|i| u.intern_type(Type::Param(binder, i as u32))));
        let avoided = self.u.map_type(body, &mut |u, t| capture(u, t).map(|_| ANY));
        Some((self.u.intern_type(Type::Lambda { binder, params, result }), avoided))
    }

    /// The names of the type variables pattern `pat` binds: its lower-case type identifiers.
    fn type_variables(&mut self, pat: NodeId) -> Vec<String> {
        let idents =
            self.ast.subtrees(pat, &|a, n| matches!(a.tree(n), Tree::Ident { name } if name.is_type));
        let mut names = Vec::new();
        for i in idents {
            let Tree::Ident { name } = *self.ast.tree(i) else { continue };
            let text = self.text(name.name).to_owned();
            if text.starts_with(|c: char| c.is_lowercase()) && !names.contains(&text) {
                names.push(text);
            }
        }
        names
    }
}
