//! Searching an implicit argument of a type (dotty's `inferImplicitArg`, `ImplicitSearch`):
//! the contextual implicits by nesting level, then the implicit scope; within one group a unique
//! winner, or one whose owner derives from the others' owners.

use super::super::Typer;
use super::candidates::Candidate;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Span, TName};
use tasty::{Type, TypeId, TypeVars};

/// How deep implicit arguments of implicit arguments may nest.
const MAX_DEPTH: u32 = 6;
/// How many candidates one search may try, nested searches included.
const MAX_TRIES: u32 = 400;

impl Typer<'_> {
    /// An implicit argument of type `pt` at `at` in `ctx`.
    pub fn implicit_arg(
        &mut self,
        pt: TypeId,
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<TNodeId, &'static str> {
        let pt = self.zonk(pt);
        if let Some(tree) = self.synthesized(pt, at)? {
            return Ok(tree);
        }
        if depth > MAX_DEPTH {
            return Err("implicit search too deep");
        }
        if depth == 0 {
            self.implicit_budget = MAX_TRIES;
        }
        let mut groups = self.contextual_implicits(ctx)?;
        groups.push(self.implicit_scope(pt));
        for group in groups {
            let mut found = Vec::new();
            for &c in &group {
                let (mark, unsupported) = (self.constraint.mark(), self.unsupported.clone());
                if self.try_implicit(c, pt, at, ctx, depth).is_ok() && self.unsupported == unsupported {
                    found.push(c);
                }
                self.constraint.rollback(mark);
                self.unsupported = unsupported;
            }
            if self.implicit_budget == 0 {
                return Err("implicit search too costly");
            }
            let winner = match found.as_slice() {
                [] => continue,
                [c] => *c,
                _ => self.most_specific(&found).ok_or("ambiguous implicit arguments")?,
            };
            return self.try_implicit(winner, pt, at, ctx, depth);
        }
        Err("no implicit argument")
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
