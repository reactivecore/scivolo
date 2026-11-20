// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/reporting/messages.scala
//! Implicit arguments no candidate can supply (dotty's `MissingImplicitArgument` without an
//! addendum), reported where neither the unit nor the library defines an implicit of the type.

use super::super::super::{Typer, errors};
use super::ImplicitClause;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::{Ast, NodeId, Tree};
use syntax::diag::Message;
use tasty::tags::{EXTENSION, GIVEN, HASDEFAULT, IMPLICIT};
use tasty::{ANY, Kind, SymbolId, Type, TypeId, Universe};

const IMPLICIT_AMBIGUOUS: &str = "scala.annotation.implicitAmbiguous";

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
        if !surely_unsupplied(t, formal) {
            return None;
        }
        let place = self.place(t, param)?;
        let shown = t.plain_text(formal)?;
        self.reported(t, errors::missing_given(&shown, &place))
    }

    /// Reports the ambiguity of the two candidates the outermost search for parameter `param` of
    /// type `formal` found, as `Nil.max` finds `Ordering.Short` and `Ordering.Int`, and returns
    /// an error tree; `None` where dotty may word the message otherwise.
    pub fn ambiguous(&self, t: &mut Typer<'_>, param: &str, formal: TypeId) -> Option<TNodeId> {
        let (a, b) = t.history.ambiguity.take()?;
        let place = self.place(t, param)?;
        let (a, b) = (candidate_shown(t, a.sym)?, candidate_shown(t, b.sym)?);
        let shown = self.formal_shown(t, formal)?;
        self.reported(t, errors::ambiguous_givens(&a, &b, &shown, &place))
    }

    /// `formal` as dotty's message shows it, the type variables of the method's type parameters
    /// that are still open shown by those parameters' names: `Ordering[B]`.
    fn formal_shown(&self, t: &mut Typer<'_>, formal: TypeId) -> Option<String> {
        let formal = t.types().zonk(formal);
        if t.types().open_vars(formal).is_empty() {
            return t.plain_text(formal);
        }
        let fun = t.tpd.strip_apply(self.tree);
        let T::TypeApply { args, .. } = *t.tpd.tree(fun) else { return None };
        let targs: Vec<TypeId> = t.tpd.items(args).iter().map(|&a| t.tpd.tpe(a)).collect();
        let targs: Vec<TypeId> = targs.into_iter().map(|a| t.types().zonk(a)).collect();
        let tparams = t.u.type_params(self.meth?);
        if targs.len() != tparams.len() {
            return None;
        }
        let named = |u: &mut Universe, any: bool| {
            u.map_type(formal, &mut |u, part| {
                let i = targs.iter().position(|&a| a == part && u.var(a).is_some())?;
                Some(if any { ANY } else { u.intern_type(Type::Sym(None, tparams[i])) })
            })
        };
        let (checked, shown) = (named(t.u, true), named(t.u, false));
        if !t.types().open_vars(checked).is_empty() {
            return None;
        }
        t.plain_text(checked)?;
        Some(t.u.show_user(shown)).filter(|text| !text.contains(['<', '?']))
    }

    /// Reports `msg` and returns an error tree if dotty reports it in this context.
    fn reported(&self, t: &mut Typer<'_>, msg: Message) -> Option<TNodeId> {
        let span = t.tpd.span(self.tree);
        if !t.certain_context(span, self.ctx) {
            return None;
        }
        t.error_at(self.at, msg);
        Some(t.error_tree(span))
    }

    /// Where dotty's message places parameter `param` of the outermost search:
    /// `parameter ord of method max in trait IterableOnceOps`.
    fn place(&self, t: &mut Typer<'_>, param: &str) -> Option<String> {
        if !t.history.open.is_empty() || param.contains('$') {
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
        Some(format!("parameter {param} of method {name}{location}"))
    }
}

/// Candidate `sym` as dotty's `showLocated` shows it: `object Short in object Ordering`; `None`
/// if it carries an `implicitAmbiguous` message or its owner is shown otherwise.
fn candidate_shown(t: &mut Typer<'_>, sym: SymbolId) -> Option<String> {
    if t.u.is_annotated(sym, IMPLICIT_AMBIGUOUS) {
        return None;
    }
    let s = t.u.symbol(sym);
    let (name, owner) = (s.name.clone(), s.owner?);
    let shown = match t.u.is_module(sym) {
        true => format!("object {name}"),
        false => t.syms.described(t.u, sym),
    };
    let location = t.location(owner).filter(|l| !l.is_empty())?;
    Some(format!("{shown}{location}"))
}

/// Whether no implicit can have type `formal`: a fully defined class type of the unit, in a
/// unit that defines and derives no implicit and annotates nothing with `implicitNotFound`.
fn surely_unsupplied(t: &mut Typer<'_>, formal: TypeId) -> bool {
    if t.u.by_name(formal).is_some() || !t.types().is_fully_defined(formal) {
        return false;
    }
    let Some(c) = t.types().direct_class(formal) else { return false };
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
