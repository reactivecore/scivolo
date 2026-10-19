//! Structural refinements `{ val a: T; def m(x: A): R }` and selections of their members
//! (dotty's `handleStructural`).

use super::implicits::conversions::Goal;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::flags::{GIVEN, IMPLICIT, MUTABLE};
use syntax::ast::{NodeId, TName, Tree};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The name and info of term refinement `r`: a value's type, a parameterless method's
    /// by-name result, a method type; `Err` for one that is not a plain value or method.
    pub fn term_refinement(&mut self, r: NodeId, ctx: Ctx) -> Result<(String, TypeId), &'static str> {
        let (name, clauses, tpt, mods) = match *self.ast.tree(r) {
            Tree::ValDef { name, tpt, mods, .. } => (name, None, tpt, mods),
            Tree::DefDef { name, paramss, tpt, mods, .. } => (name, Some(paramss), tpt, mods),
            _ => return Err("structural refinement"),
        };
        if self.ast.modifiers(mods).flags.is_one_of(MUTABLE.or(GIVEN).or(IMPLICIT)) {
            return Err("structural refinement with modifiers");
        }
        let text = self.text(name.name).to_owned();
        let result = self.typed_type(tpt, ctx);
        let mut info = self.tpd.tpe(result);
        if matches!(self.u.types().get(info), Type::Error) {
            return Err("structural refinement of an unsupported type");
        }
        let Some(clauses) = clauses else { return Ok((text, info)) };
        let clauses = self.ast.clause_items(clauses).to_vec();
        if clauses.is_empty() {
            return Ok((text, self.u.intern_type(Type::ByName(info))));
        }
        for &clause in clauses.iter().rev() {
            let mut params = Vec::new();
            for &p in self.ast.items(clause).to_vec().iter() {
                let Tree::ValDef { name, tpt, mods, .. } = *self.ast.tree(p) else {
                    return Err("polymorphic structural refinement");
                };
                if self.ast.modifiers(mods).flags.is_one_of(GIVEN.or(IMPLICIT)) {
                    return Err("structural refinement with a context parameter");
                }
                if matches!(self.ast.tree(tpt), Tree::ByNameTypeTree { .. }) {
                    return Err("structural refinement with a by-name parameter");
                }
                let typed = self.typed_type(tpt, ctx);
                let tpe = self.tpd.tpe(typed);
                if matches!(self.u.types().get(tpe), Type::Error) {
                    return Err("structural refinement of an unsupported type");
                }
                params.push((self.text(name.name).to_owned(), tpe));
            }
            let binder = self.syms.fresh_binder();
            let method = Type::Method { binder, params, result: info, implicit: false, contextual: false };
            info = self.u.intern_type(method);
        }
        Ok((text, info))
    }

    /// Selection `id` of member `name` of `qual`, which only a term refinement of its type `pre`
    /// declares: `(qual: Selectable).selectDynamic("name").$asInstanceOf$[T]`; `None` if no
    /// refinement declares it.
    pub fn structural_select(
        &mut self,
        id: NodeId,
        (qual, pre): (TNodeId, TypeId),
        name: TName,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        let text = self.text(name.name).to_owned();
        if name.is_type {
            return None;
        }
        let info = self.u.term_refinement_named(pre, &text)?;
        if matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. }) {
            return Some(self.unsupported(id, "structural method call"));
        }
        let tpe = self.u.intern_type(Type::TermRef(pre, text.clone()));
        let selectable = self.u.class_ref("scala.Selectable");
        let qual_tpe = self.zonk(self.tpd.tpe(qual));
        let qual = if self.u.is_subtype(qual_tpe, selectable) {
            qual
        } else {
            match self.converted(qual, Goal::Conform(selectable), ctx) {
                Ok(Some(q)) => q,
                Ok(None) => {
                    return Some(self.unsupported(id, "structural selection without reflectiveSelectable"));
                }
                Err(what) => return Some(self.unsupported(id, what)),
            }
        };
        let span = self.ast.span(id);
        let qualifier = self.spliced(qual);
        let dyn_name = TName { name: self.name("selectDynamic"), is_type: false };
        let fun = self.ast.node(Tree::Select { qualifier, name: dyn_name });
        self.ast.with_span(fun, span);
        let literal = self.string_literal(&text, span.end_pos());
        let (apply, args) = self.applied(fun, &[literal]);
        let call = self.typed_apply(apply, fun, args, Pt::Any, ctx);
        if self.is_error(call) {
            return Some(call);
        }
        Some(self.cast_to_captured(call, tpe).unwrap_or_else(|what| self.unsupported(id, what)))
    }
}
