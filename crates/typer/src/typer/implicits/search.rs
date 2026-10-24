// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Searching an implicit argument of a type (dotty's `inferImplicitArg`, `ImplicitSearch`, `isCoherent`):
//! the contextual implicits by nesting level, then the implicit scope, then synthesis; within
//! one group a unique winner or the most specific one. A candidate that does not fit is skipped;
//! any other failure of a candidate fails the search.

use super::super::{Pt, Typer};
use super::candidates::Candidate;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::{Type, TypeId};

/// How deep implicit arguments of implicit arguments may nest.
const MAX_DEPTH: u32 = 6;
/// How many candidates one search may try, nested searches included.
pub const MAX_TRIES: u32 = 400;
/// The refusal of a search whose winner among several leaves type variables to instantiate.
const OPEN: &str = "implicit search for a type with type variables";

/// The failures that only mean a candidate does not fit.
pub const MISFITS: [&str; 9] = [
    "implicit of another type",
    "inaccessible implicit",
    "diverging implicit",
    "not an implicit value",
    "no implicit argument",
    "not a conversion",
    "conversion of another type",
    "conversion to another type",
    "type argument outside its bounds",
];

pub fn is_misfit(what: &str) -> bool {
    MISFITS.contains(&what)
}

impl Typer<'_> {
    /// An implicit argument of type `pt` at `at` in `ctx`, with the dictionary of its by-name
    /// implicits if the search is outermost; the constraint knows the search is in progress.
    pub fn implicit_arg(
        &mut self,
        pt: TypeId,
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<TNodeId, &'static str> {
        let outermost = self.history.open.is_empty();
        self.constraint.enter_search();
        let found = match self.var_upper_bound(pt) {
            Some(hi) => self.arg_search(hi, at, ctx, depth).and_then(|t| {
                let tpe = self.tpd.tpe(t);
                if self.conforms(tpe, pt) { Ok(t) } else { Err("implicit of another type") }
            }),
            None => self.arg_search(pt, at, ctx, depth),
        };
        self.constraint.leave_search();
        if outermost { self.emitted(found, at, ctx) } else { found }
    }

    fn arg_search(&mut self, pt: TypeId, at: Span, ctx: Ctx, depth: u32) -> Result<TNodeId, &'static str> {
        let (pt, by_name) = match *self.u.types().get(pt) {
            Type::ByName(r) => (r, true),
            _ => (pt, false),
        };
        let pt = self.determined(pt);
        if let Some(what) = self.unsettled(pt) {
            return Err(what);
        }
        if depth > MAX_DEPTH {
            return Err("implicit search too deep");
        }
        if depth == 0 && self.history.open.is_empty() {
            self.implicit_budget = MAX_TRIES;
        }
        if let Some(found) = self.recursive_ref(pt, by_name, at)? {
            return Ok(found);
        }
        if let Some(negated) = self.not_given_arg(pt) {
            return self.not_given(negated, at, ctx, depth);
        }
        if self.is_underspecified(pt) {
            return Err("no implicit argument");
        }
        let coherent = self.class_args(pt, "scala.CanEqual").is_some();
        let contextual = self.contextual_implicits(ctx, &mut |t, c| t.is_eligible(c, pt))?;
        let levels = contextual.len();
        let mut contextual = contextual.into_iter();
        for _ in 0..=levels {
            let group = match contextual.next() {
                Some(group) => group,
                None => self.implicit_scope(pt)?,
            };
            let mut nested_failure = false;
            let found = self.fitting(&group, |t, c| {
                let tried = t.tried(c, (pt, by_name), at, ctx, depth);
                nested_failure |= tried == Err("no implicit argument");
                tried
            })?;
            let winner = match found.as_slice() {
                [] => continue,
                [_] if nested_failure && self.has_unbounded_var(pt) => {
                    return Err("implicit candidate rejected by a search with type variables");
                }
                [c] => *c,
                _ if coherent && !self.is_fully_defined(pt) => return Err(OPEN),
                [c, ..] if coherent => *c,
                _ => self.most_specific(&found, ctx).ok_or("ambiguous implicit arguments")?,
            };
            return self.tried(winner, (pt, by_name), at, ctx, depth);
        }
        match self.synthesized(pt, at, ctx, depth)? {
            Some(tree) => Ok(tree),
            None => Err("no implicit argument"),
        }
    }

    /// The type arguments of `t` if it refers to class `name`.
    pub fn class_args(&mut self, t: TypeId, name: &str) -> Option<Vec<TypeId>> {
        let (tycon, args) = match self.u.types().get(t).clone() {
            Type::Applied(tycon, args) => (tycon, args),
            _ => (t, Vec::new()),
        };
        let class = self.u.class_symbol(tycon)?;
        (self.u.full_name(class) == name).then_some(args)
    }

    /// Candidate `c`, an implicit that may fit, as an argument of type `pt`: referenced, applied
    /// to type variables and to its own implicit arguments, eta-expanded if it is a method and
    /// `pt` a function type.
    pub fn try_implicit(
        &mut self,
        c: Candidate,
        pt: TypeId,
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<TNodeId, &'static str> {
        let from = self.constraint.count();
        let tree = self.candidate_ref(c, at, ctx)?;
        let tree = self.with_type_vars(tree)?;
        let t = self.tpd.tpe(tree);
        let info = match *self.u.types().get(t) {
            Type::Sym(_, s) if self.u.is_type_symbol(s) => t,
            _ => self.method_info(t).unwrap_or(t),
        };
        let stripped = self.strip_implicit(info);
        let method = matches!(self.u.types().get(stripped), Type::Method { .. });
        match self.implicit_value_type(info, c.sym, pt) {
            Some(value) if self.conforms(value, pt) => {}
            _ => return Err("implicit of another type"),
        }
        if self.implicit_budget == 0 {
            return Err("implicit search too costly");
        }
        self.implicit_budget -= 1;
        let applied = self.with_implicit_args(tree, (at, at), ctx, depth + 1)?;
        if applied != tree {
            self.interpolate(applied, Pt::Type(pt), from)?;
        }
        let tree = applied;
        let tree = if method { self.eta_expanded_implicit(tree, pt, ctx)? } else { tree };
        let tree = self.inlined_transparent(tree, Pt::Type(pt), ctx).unwrap_or(tree);
        self.record_inline_call(tree, ctx);
        let t = self.tpd.tpe(tree);
        let info = self.method_info(t).unwrap_or(t);
        if matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. }) {
            return Err("not an implicit value");
        }
        if self.conforms(t, pt) { Ok(tree) } else { Err("implicit of another type") }
    }
}
