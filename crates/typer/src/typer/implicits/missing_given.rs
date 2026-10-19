// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/reporting/messages.scala
//! Implicit arguments no candidate can supply (dotty's `MissingImplicitArgument` without an
//! addendum), reported where neither the unit nor the library defines an implicit of the type.

use super::super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::flags as f;
use syntax::ast::{Ast, NodeId, Span, Tree};
use tasty::tags::{EXTENSION, GIVEN, HASDEFAULT, IMPLICIT};
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// `MissingImplicitArgument` at `at` for parameter `param` of type `formal` of the method
    /// `tree` refers to, typed in `ctx`, an error tree; `None` where dotty may find a candidate,
    /// add an addendum or word the message otherwise.
    pub fn missing_given(
        &mut self,
        tree: TNodeId,
        (param, formal): (&str, TypeId),
        at: Span,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        let span = self.tpd.span(tree);
        if !self.history.open.is_empty() || param.contains('$') || !self.surely_unsupplied(formal) {
            return None;
        }
        let m = self.tpd.sym(self.tpd.meth_part(tree))?;
        let sym = self.u.symbol(m);
        let (name, owner, flags) = (sym.name.clone(), sym.owner?, sym.flags);
        if sym.kind != Kind::Def
            || name.contains(['$', '<'])
            || [GIVEN, IMPLICIT, EXTENSION].into_iter().any(|f| flags.has(f))
            || sym.decls.iter().any(|&p| self.u.symbol(p).flags.has(HASDEFAULT))
        {
            return None;
        }
        let location = self.location(owner)?;
        let shown = self.plain_text(formal)?;
        if !self.certain_context(span, ctx) {
            return None;
        }
        let msg = super::super::errors::missing_given(
            &shown,
            &format!("parameter {param} of method {name}{location}"),
        );
        self.error_at(at, msg);
        Some(self.error_tree(span))
    }

    /// Whether no implicit can have type `formal`: a fully defined class type of the unit, in a
    /// unit that defines and derives no implicit and annotates nothing with `implicitNotFound`.
    fn surely_unsupplied(&mut self, formal: TypeId) -> bool {
        if matches!(self.u.types().get(formal), Type::ByName(_)) || !self.is_fully_defined(formal) {
            return false;
        }
        let Some(c) = self.direct_class(formal) else { return false };
        let unit_class = self.syms.flags.contains_key(&c) && !self.u.is_module_class(c);
        let special = |ast: &Ast, n: NodeId| match *ast.tree(n) {
            Tree::Template { derived: Some(_), .. } => true,
            Tree::Ident { name } | Tree::Select { name, .. } => self.text(name.name) == "implicitNotFound",
            _ => ast.mods_of(n).is_some_and(|m| {
                let flags = ast.modifiers(m).flags;
                flags.is(f::GIVEN_OR_IMPLICIT) && !flags.is(f::PARAM)
            }),
        };
        unit_class && !self.ast.exists_subtree(self.unit, &special)
    }
}
