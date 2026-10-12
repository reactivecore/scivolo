//! `summonFrom { cases }`: an inline match without a selector, its patterns typed against the
//! implicit scrutinee and their variables givens (dotty's `typedMatch` of an `InlineMatch` with
//! an empty selector, `ImplicitScrutineeTypeRef`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::modifiers::Modifiers;
use syntax::ast::tree::MatchKind;
use syntax::ast::{List, NodeId, TName, Tree};
use tasty::{Flags, Kind, NOTHING, Type, TypeId};

impl Typer<'_> {
    pub fn typed_summon_from(&mut self, id: NodeId, cases: List, pt: Pt, ctx: Ctx) -> TNodeId {
        let span = self.ast.span(id);
        self.check_in_inline_context("summonFrom", span, ctx);
        let scrutinee = self.implicit_scrutinee();
        let case_pt = match pt {
            Pt::Fun(_) | Pt::PolyFun => Pt::Any,
            pt => pt,
        };
        let mut typed = Vec::new();
        for c in self.ast.items(cases).to_vec() {
            let Tree::CaseDef { pat, guard, body } = *self.ast.tree(c) else {
                return self.unsupported(c, "case");
            };
            let c = match self.summon_pattern(pat) {
                Some(p) if p == pat => c,
                Some(p) => {
                    let case = self.ast.node(Tree::CaseDef { pat: p, guard, body });
                    self.ast.with_span(case, self.ast.span(c));
                    case
                }
                None => return self.unsupported(pat, "summonFrom pattern"),
            };
            typed.push(self.typed_case(c, scrutinee, case_pt, ctx));
        }
        let mut tpe = NOTHING;
        for &c in &typed {
            let T::CaseDef { body, .. } = *self.tpd.tree(c) else { return self.unsupported(id, "case") };
            let body = self.tpd.tpe(body);
            tpe = self.u.lub(tpe, body);
        }
        let cases = self.tpd.list(&typed);
        self.tpd.node(T::Match { selector: None, cases, kind: MatchKind::Inline }, span, Some(tpe), None)
    }

    /// Pattern `pat` of a `summonFrom` case as it is typed: `_: T` bound to a fresh name; `None`
    /// if it is neither a typed identifier, a bind of one nor `_`.
    fn summon_pattern(&mut self, pat: NodeId) -> Option<NodeId> {
        let is_ident = |t: &Self, n: NodeId| matches!(t.ast.tree(n), Tree::Ident { .. });
        match *self.ast.tree(pat) {
            Tree::Typed { expr, .. } if is_ident(self, expr) => {
                let Tree::Ident { name } = *self.ast.tree(expr) else { return None };
                if self.text(name.name) != "_" {
                    return Some(pat);
                }
                let name = TName { name: self.fresh.next(self.names, "_$"), is_type: false };
                let mods = self.ast.mods(Modifiers::default());
                let bind = self.ast.node(Tree::Bind { name, body: pat, mods });
                self.ast.with_span(bind, self.ast.span(pat).to_synthetic());
                Some(bind)
            }
            Tree::Bind { body, .. } if self.is_typed_ident(body) => Some(pat),
            Tree::Ident { name } if self.text(name.name) == "_" => Some(pat),
            _ => None,
        }
    }

    fn is_typed_ident(&self, t: NodeId) -> bool {
        matches!(*self.ast.tree(t), Tree::Typed { expr, .. } if matches!(self.ast.tree(expr), Tree::Ident { .. }))
    }

    /// The type of the scrutinee of `summonFrom`, `scala.implicit`, defined on first use.
    fn implicit_scrutinee(&mut self) -> TypeId {
        if let Some(t) = self.syms.implicit_scrutinee {
            return t;
        }
        let sym = crate::defn::special(self.u, self.defn.scala, "implicit", Kind::Type, Flags::default());
        let bounds = self.u.intern_type(Type::Bounds(NOTHING, tasty::ANY));
        self.u.set_info(sym, bounds);
        let t = crate::defn::type_ref(self.u, sym);
        self.syms.implicit_scrutinee = Some(t);
        t
    }
}
