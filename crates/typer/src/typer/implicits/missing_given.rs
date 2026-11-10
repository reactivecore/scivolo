// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/reporting/messages.scala
//! Implicit arguments no candidate can supply (dotty's `MissingImplicitArgument` without an
//! addendum), reported where neither the unit nor the library defines an implicit of the type.

use super::super::Typer;
use super::super::errors;
use super::implicit_clause::ImplicitClause;
use crate::tpd::TNodeId;
use syntax::ast::flags as f;
use syntax::ast::{Ast, NodeId, Tree};
use tasty::tags::{EXTENSION, GIVEN, HASDEFAULT, IMPLICIT};
use tasty::{Kind, Type, TypeId};

impl ImplicitClause {
    /// Reports that no implicit argument was found for parameter `param` of type `formal` of an
    /// outermost search and returns an error tree, where dotty reports it with the same text;
    /// `None` leaves the failure to the caller.
    pub fn unsupplied(&self, t: &mut Typer<'_>, param: &str, formal: TypeId) -> Option<TNodeId> {
        if let Some(error) = self.missing_given(t, param, formal) {
            return Some(error);
        }
        if !t.history.open.is_empty() || !t.unsupplied_in_checked_code(formal, self.meth, self.ctx) {
            return None;
        }
        let shown = t.plain_text(formal).unwrap_or_default();
        let place = format!("parameter {param}");
        t.error_at(self.at, errors::missing_given(&shown, &place));
        Some(t.error_tree(t.tpd.span(self.tree)))
    }

    /// Reports `MissingImplicitArgument` for parameter `param` of type `formal` of the method the
    /// tree refers to, as for `def f(using Config) = ...; f` where no `Config` is defined
    /// anywhere, and returns an error tree; `None` where dotty may find a candidate, add an
    /// addendum or word the message otherwise.
    fn missing_given(&self, t: &mut Typer<'_>, param: &str, formal: TypeId) -> Option<TNodeId> {
        let span = t.tpd.span(self.tree);
        if !t.history.open.is_empty() || param.contains('$') || !surely_unsupplied(t, formal) {
            return None;
        }
        let m = t.tpd.sym(t.tpd.meth_part(self.tree))?;
        let sym = t.u.symbol(m);
        let (name, owner, flags) = (sym.name.clone(), sym.owner?, sym.flags);
        if sym.kind != Kind::Def
            || name.contains(['$', '<'])
            || [GIVEN, IMPLICIT, EXTENSION].into_iter().any(|f| flags.has(f))
            || sym.decls.iter().any(|&p| t.u.symbol(p).flags.has(HASDEFAULT))
        {
            return None;
        }
        let location = t.location(owner)?;
        let shown = t.plain_text(formal)?;
        if !t.certain_context(span, self.ctx) {
            return None;
        }
        let msg = errors::missing_given(&shown, &format!("parameter {param} of method {name}{location}"));
        t.error_at(self.at, msg);
        Some(t.error_tree(span))
    }
}

/// Whether no implicit can have type `formal`: a fully defined class type of the unit, in a
/// unit that defines and derives no implicit and annotates nothing with `implicitNotFound`.
fn surely_unsupplied(t: &mut Typer<'_>, formal: TypeId) -> bool {
    if matches!(t.u.types().get(formal), Type::ByName(_)) || !t.types().is_fully_defined(formal) {
        return false;
    }
    let Some(c) = t.direct_class(formal) else { return false };
    let unit_class = t.syms.flags.contains_key(&c) && !t.u.is_module_class(c);
    let special = |ast: &Ast, n: NodeId| match *ast.tree(n) {
        Tree::Template { derived: Some(_), .. } => true,
        Tree::Ident { name } | Tree::Select { name, .. } => t.text(name.name) == "implicitNotFound",
        _ => ast.mods_of(n).is_some_and(|m| {
            let flags = ast.modifiers(m).flags;
            flags.is(f::GIVEN_OR_IMPLICIT) && !flags.is(f::PARAM)
        }),
    };
    unit_class && !t.ast.exists_subtree(t.unit, &special)
}
