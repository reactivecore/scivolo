//! Searching an implicit argument of a type (dotty's `inferImplicitArg`, `ImplicitSearch`):
//! the contextual implicits by nesting level, then the implicit scope, then synthesis; within
//! one group a unique winner or the most specific one. A candidate that does not fit is skipped;
//! any other failure of a candidate fails the search.

use super::super::Typer;
use super::candidates::Candidate;
use crate::ctx::Ctx;
use crate::symbols::Completion;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use tasty::{Type, TypeId, TypeVars};

/// How deep implicit arguments of implicit arguments may nest.
const MAX_DEPTH: u32 = 6;
/// How many candidates one search may try, nested searches included.
pub const MAX_TRIES: u32 = 400;

/// The failures that only mean a candidate does not fit.
const MISFITS: [&str; 7] = [
    "implicit of another type",
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
    /// An implicit argument of type `pt` at `at` in `ctx`.
    pub fn implicit_arg(
        &mut self,
        pt: TypeId,
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<TNodeId, &'static str> {
        let pt = self.determined(pt);
        if depth > MAX_DEPTH {
            return Err("implicit search too deep");
        }
        if depth == 0 {
            self.implicit_budget = MAX_TRIES;
        }
        let mut groups = self.contextual_implicits(ctx)?;
        groups.push(self.implicit_scope(pt));
        for group in groups {
            let found = self.fitting(&group, |t, c| t.try_implicit(c, pt, at, ctx, depth))?;
            let winner = match found.as_slice() {
                [] => continue,
                [c] => *c,
                _ => self.most_specific(&found).ok_or("ambiguous implicit arguments")?,
            };
            return self.try_implicit(winner, pt, at, ctx, depth);
        }
        match self.synthesized(pt, at)? {
            Some(tree) => Ok(tree),
            None => Err("no implicit argument"),
        }
    }

    /// `t` with its type variables whose bounds are equal instantiated to them.
    fn determined(&mut self, t: TypeId) -> TypeId {
        let t = self.zonk(t);
        let mut vars = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Type::Var(v) = *u.types().get(part) {
                vars.push(v);
            }
            None
        });
        for v in vars {
            let (lo, hi) = self.constraint.bounds(v);
            if lo == hi && self.constraint.instance(v).is_none() {
                self.constraint.instantiate(v, lo);
            }
        }
        self.zonk(t)
    }

    /// The candidates of `group` that `attempt` accepts, each tried with the constraint and the
    /// recorded fallback restored afterwards; `Err` if one fails other than by not fitting.
    pub fn fitting(
        &mut self,
        group: &[Candidate],
        mut attempt: impl FnMut(&mut Self, Candidate) -> Result<TNodeId, &'static str>,
    ) -> Result<Vec<Candidate>, &'static str> {
        let mut found = Vec::new();
        for &c in group {
            if matches!(self.syms.completions.get(&c.sym), Some(Completion::Completing(_))) {
                return Err("implicit candidate whose definition is being completed");
            }
            let (mark, unsupported) = (self.constraint.mark(), self.unsupported.clone());
            let result = attempt(self, c);
            let clean = self.unsupported == unsupported;
            self.constraint.rollback(mark);
            self.unsupported = unsupported;
            if !clean {
                return Err("implicit candidate outside the subset");
            }
            match result {
                Ok(_) => found.push(c),
                Err(what) if is_misfit(what) => {}
                Err(what) => return Err(what),
            }
        }
        Ok(found)
    }

    /// Candidate `c` as an argument of type `pt`: referenced, applied to type variables and to
    /// its own implicit arguments.
    fn try_implicit(
        &mut self,
        c: Candidate,
        pt: TypeId,
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<TNodeId, &'static str> {
        if self.implicit_budget == 0 {
            return Err("implicit search too costly");
        }
        self.implicit_budget -= 1;
        self.complete(c.sym);
        let name = self.u.symbol(c.sym).name.clone();
        let name = TName { name: self.name(&name), is_type: false };
        let tpe = self.ref_type(c.prefix, c.sym);
        let tree = self.tpd.node(T::Ident { name }, at, Some(tpe), Some(c.sym));
        let mut tree = self.with_type_vars(tree)?;
        let t = self.tpd.tpe(tree);
        let mut result = self.method_info(t).unwrap_or(t);
        while let Type::Method { result: r, implicit: true, .. } = *self.u.types().get(result) {
            result = r;
        }
        if matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. })
            || !self.conforms(result, pt)
        {
            return Err("implicit of another type");
        }
        loop {
            let t = self.tpd.tpe(tree);
            let info = self.method_info(t).unwrap_or(t);
            match self.u.types().get(info).clone() {
                Type::Method { params, result, implicit: true, .. } => {
                    let mut args = Vec::new();
                    for (_, p) in params {
                        args.push(self.implicit_arg(p, at, ctx, depth + 1)?);
                    }
                    let args = self.tpd.list(&args);
                    tree = self.tpd.node(T::Apply { fun: tree, args }, at, Some(result), None);
                }
                Type::Method { .. } | Type::Poly { .. } => return Err("not an implicit value"),
                _ => break,
            }
        }
        let t = self.tpd.tpe(tree);
        if self.conforms(t, pt) { Ok(tree) } else { Err("implicit of another type") }
    }
}
