// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Typing a candidate as an implicit argument (dotty's `typedImplicit`), and implicit methods as
//! values of a function type (dotty's `candidateKind` for a `FunctionN` goal).

use super::super::{Pt, Typer};
use super::candidates::{Candidate, candidate_ref};
use super::search::{ImplicitSearch, MISFITS};
use crate::tpd::TNodeId;
use syntax::diag::Level;
use tasty::{SymbolId, Type, TypeId};

/// Type `tpe` of implicit `sym` as a value compared with `pt`: its implicit clauses stripped,
/// references to their parameters made wildcards, a method as a function type if `pt` is a
/// `FunctionN` type (dotty's `normalize`); `None` if it stays a method.
pub fn implicit_value_type(t: &mut Typer<'_>, tpe: TypeId, sym: SymbolId, pt: TypeId) -> Option<TypeId> {
    let tpe = match t.u.ty(tpe).clone() {
        Type::Method { binder, params, result, implicit: true, contextual } => {
            let result = t.types().without_false_deps_in(result, (binder, &params), Some(sym));
            let tpe = t.u.intern_type(Type::Method { binder, params, result, implicit: true, contextual });
            let stripped = t.strip_implicit_clauses(tpe, sym).0;
            wildcard_selections(t.u, stripped)
        }
        _ => tpe,
    };
    let method = t.u.method(tpe).is_some();
    let tpe = match method && t.types().is_function_type(pt) {
        true => t.proto_normalized((tpe, Some(sym)), pt, true, 8)?,
        false => tpe,
    };
    (!matches!(t.u.ty(tpe), Type::Method { .. } | Type::Poly { .. })).then_some(tpe)
}

/// Whether implicit `sym` of type `tpe` fits `pt`: `Some(true)` if its value type conforms,
/// `Some(false)` if it does once context function types on either side are replaced by their
/// results (dotty's `normalize` following them).
pub fn implicit_fit(t: &mut Typer<'_>, tpe: TypeId, sym: SymbolId, pt: TypeId) -> Option<bool> {
    let value = implicit_value_type(t, tpe, sym, pt)?;
    if t.types().conforms(value, pt) {
        return Some(true);
    }
    let (result, pt_result) = (t.context_result(value), t.context_result(pt));
    ((result != value || pt_result != pt) && t.types().conforms(result, pt_result)).then_some(false)
}

impl ImplicitSearch {
    /// Candidate `c`, an implicit that may fit, as an argument: referenced, applied to type
    /// variables and to its own implicit arguments, eta-expanded if it is a method and `pt` a
    /// function type. If `shallow`, the reference once its type fits.
    pub fn try_implicit(
        &self,
        t: &mut Typer<'_>,
        c: Candidate,
        shallow: bool,
    ) -> Result<TNodeId, &'static str> {
        let (pt, at, ctx) = (self.pt, self.at, self.ctx);
        let from = t.constraint.count();
        let tree = candidate_ref(t, c, at, ctx)?;
        let tree = t.with_type_vars(tree)?;
        let tpe = t.tpd.tpe(tree);
        let info = match t.u.sym_of(tpe) {
            Some(s) if t.u.is_type_symbol(s) => tpe,
            _ => t.method_info(tpe).unwrap_or(tpe),
        };
        let stripped = t.types().strip_implicit(info);
        let method = t.u.method(stripped).is_some();
        let contextual = t.context_function_parts(pt).is_some();
        let direct = implicit_fit(t, info, c.sym, pt).ok_or("implicit of another type")?;
        if shallow {
            return Ok(tree);
        }
        if t.history.budget == 0 {
            return Err("implicit search too costly");
        }
        t.history.budget -= 1;
        if contextual {
            return self.contextual_implicit(t, tree);
        }
        let applied = t.with_implicit_args(tree, (at, at), ctx, self.depth + 1)?;
        if applied != tree {
            t.interpolate_zonked(applied, Pt::Type(pt), from)?;
        }
        let tree = match (method, direct) {
            (true, _) => self.eta_expanded_implicit(t, applied)?,
            (false, false) => self.context_applied_implicit(t, applied)?,
            (false, true) => applied,
        };
        let tree = t.inlined_transparent(tree, Pt::Type(pt), ctx).unwrap_or(tree);
        t.record_inline_call(tree, ctx);
        t.record_implicit_type_quote(tree, ctx);
        let tpe = t.tpd.tpe(tree);
        let info = t.method_info(tpe).unwrap_or(tpe);
        if matches!(t.u.ty(info), Type::Method { .. } | Type::Poly { .. }) {
            return Err("not an implicit value");
        }
        let fits = t.types().conforms(tpe, pt);
        fits.then(|| t.gadt_adapted(tree, pt, ctx)).ok_or("implicit of another type")
    }

    /// Method reference `tree` eta-expanded to function type `pt`; a misfit if it refers to an
    /// inline implicit method, typing the function reports an error, or fails by a misfit.
    pub fn eta_expanded_implicit(&self, t: &mut Typer<'_>, tree: TNodeId) -> Result<TNodeId, &'static str> {
        if t.is_inline_implicit_ref(tree) {
            return Err("implicit of another type");
        }
        typed_as_implicit(t, |t| t.adapt(tree, Pt::Type(self.pt), self.ctx))
    }

    /// `tree`, a value of a context function type, applied to its implicit arguments for `pt`.
    pub fn context_applied_implicit(
        &self,
        t: &mut Typer<'_>,
        tree: TNodeId,
    ) -> Result<TNodeId, &'static str> {
        typed_as_implicit(t, |t| match t.context_function_applied(tree, Pt::Type(self.pt), self.ctx) {
            Some(Ok(applied)) => applied,
            Some(Err(what)) => t.unsupported_tree(t.tpd.span(tree), what),
            None => tree,
        })
    }

    /// Candidate reference `tree` as an argument of context function type `pt`: a context
    /// closure around it typed with its parameters in scope (dotty's `typedImplicit` of a context
    /// function goal).
    pub fn contextual_implicit(&self, t: &mut Typer<'_>, tree: TNodeId) -> Result<TNodeId, &'static str> {
        let splice = t.ast.node(syntax::ast::Tree::TypeTree);
        t.ast.with_span(splice, t.tpd.span(tree));
        t.splices.insert(splice, tree);
        let closure = typed_as_implicit(t, |t| t.typed(splice, Pt::Type(self.pt), self.ctx))?;
        let tpe = t.tpd.tpe(closure);
        if t.types().conforms(tpe, self.pt) { Ok(closure) } else { Err("implicit of another type") }
    }
}

/// The tree `typing` gives; a misfit if it reports an error or fails by a misfit.
fn typed_as_implicit<'a>(
    t: &mut Typer<'a>,
    typing: impl FnOnce(&mut Typer<'a>) -> TNodeId,
) -> Result<TNodeId, &'static str> {
    let (outer, diags) = (t.unsupported.take(), t.diags.items.len());
    let adapted = typing(t);
    let inner = std::mem::replace(&mut t.unsupported, outer);
    if t.diags.items[diags..].iter().any(|d| d.level == Level::Error) {
        t.diags.items.truncate(diags);
        return Err("implicit of another type");
    }
    let Some((span, what)) = inner else { return Ok(adapted) };
    if let Some(misfit) = MISFITS.iter().find(|&&m| m == what) {
        return Err(misfit);
    }
    t.unsupported.get_or_insert((span, what));
    Err("implicit method as a function value")
}

/// `t` with every selection from a wildcard made a wildcard (dotty's `derivedSelect` of a
/// `WildcardType` prefix).
fn wildcard_selections(u: &mut tasty::Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, x| {
        let (Type::TypeRef(p, _) | Type::TermRef(p, _) | Type::Sym(Some(p), _)) = *u.ty(x) else {
            return None;
        };
        let p = wildcard_selections(u, p);
        matches!(u.ty(p), Type::Wildcard(_)).then(|| u.intern_type(Type::Wildcard(None)))
    })
}
