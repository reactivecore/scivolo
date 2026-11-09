// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Searching an implicit argument of a type (dotty's `inferImplicitArg`, `ImplicitSearch`, `isCoherent`):
//! the contextual implicits by nesting level, then the implicit scope, then synthesis; within
//! one group a unique winner or the most specific one. A candidate that does not fit is skipped;
//! any other failure of a candidate fails the search.

use super::super::Typer;
use super::attempts::{BestFirst, fitting};
use super::candidates::Candidate;
use super::history::emitted;
use super::not_given::not_given_arg;
use super::ranking::most_specific;
use super::unsettled::{
    any_open_constructor, constructor_vars, determined, has_unbounded_var, is_underspecified, unsettled,
    var_upper_bound,
};
use crate::ctx::{Ctx, Mode};
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::{SymbolId, Type, TypeId};

/// How deep implicit arguments of implicit arguments may nest.
const MAX_DEPTH: u32 = 12;
/// How many candidates one search may try, nested searches included.
pub const MAX_TRIES: u32 = 4000;
/// The failure of a search that finds several candidates, none ranking above the others.
pub const AMBIGUOUS: &str = "ambiguous implicit arguments";

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

/// One search of an implicit argument (dotty's `ImplicitSearch`); the searches in progress and
/// the by-name dictionary stay in `Typer::history`, which nested searches share.
pub struct ImplicitSearch {
    /// The type searched for, without its by-name, its determined type variables instantiated.
    pub pt: TypeId,
    pub by_name: bool,
    pub at: Span,
    pub ctx: Ctx,
    /// How deep the search nests in implicit arguments of implicit arguments.
    pub depth: u32,
    /// The candidates the search leaves out.
    ignored: Vec<SymbolId>,
    /// Whether the search is for a `CanEqual` (dotty's `isCoherent`).
    coherent: bool,
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
        ImplicitSearch::new(pt, at, ctx, depth, Vec::new()).run(self)
    }

    /// `implicit_arg` of an outermost search that leaves out the candidates `ignored`.
    pub fn implicit_arg_ignoring(
        &mut self,
        (pt, ignored): (TypeId, &[SymbolId]),
        at: Span,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        ImplicitSearch::new(pt, at, ctx, 0, ignored.to_vec()).run(self)
    }
}

impl ImplicitSearch {
    fn new(pt: TypeId, at: Span, ctx: Ctx, depth: u32, ignored: Vec<SymbolId>) -> Self {
        ImplicitSearch { pt, by_name: false, at, ctx, depth, ignored, coherent: false }
    }

    fn run(mut self, t: &mut Typer<'_>) -> Result<TNodeId, &'static str> {
        let (formal, at, ctx) = (self.pt, self.at, self.ctx);
        let outermost = t.history.open.is_empty();
        t.constraint.enter_search();
        let constructors = constructor_vars(t.u, formal);
        let found = match var_upper_bound(&mut t.types(), formal) {
            Some(hi) => {
                self.pt = hi;
                self.search(t).and_then(|tree| {
                    let tpe = t.tpd.tpe(tree);
                    if t.types().conforms(tpe, formal) { Ok(tree) } else { Err("implicit of another type") }
                })
            }
            None => self.search(t),
        };
        t.constraint.leave_search();
        if found.is_ok() && any_open_constructor(&t.constraint, &constructors) {
            return Err("higher-kinded type variable left without lower bound by an implicit search");
        }
        if outermost { emitted(t, found, at, ctx) } else { found }
    }

    /// The argument of the first group of candidates that gives one, else a synthesized one.
    fn search(&mut self, t: &mut Typer<'_>) -> Result<TNodeId, &'static str> {
        if let Type::ByName(r) = *t.u.types().get(self.pt) {
            (self.pt, self.by_name) = (r, true);
        }
        self.pt = determined(&mut t.types(), self.pt);
        let (pt, ctx) = (self.pt, self.ctx);
        if let Some(what) = unsettled(&mut t.types(), pt) {
            return Err(what);
        }
        if self.depth > MAX_DEPTH {
            return Err("implicit search too deep");
        }
        if self.depth == 0 && t.history.open.is_empty() {
            t.history.budget = MAX_TRIES;
        }
        if let Some(found) = self.recursive_ref(t)? {
            return Ok(found);
        }
        if let Some(negated) = not_given_arg(&mut t.types(), pt) {
            return self.not_given(t, negated);
        }
        if is_underspecified(&mut t.types(), pt) {
            return Err("no implicit argument");
        }
        self.coherent = t.types().class_args(pt, "scala.CanEqual").is_some();
        let contextual = t.contextual_implicits(ctx, &mut |t, c| self.is_eligible(t, c))?;
        let levels = contextual.len();
        let mut contextual = contextual.into_iter();
        for _ in 0..=levels {
            let mut group = match contextual.next() {
                Some(group) => group,
                None => t.implicit_scope(pt)?,
            };
            group.retain(|c| !self.ignored.contains(&c.sym));
            if let Some(found) = self.in_group(t, group)? {
                return Ok(found);
            }
        }
        match t.synthesized(pt, self.at, ctx, self.depth)? {
            Some(tree) => Ok(tree),
            None => Err("no implicit argument"),
        }
    }

    /// The argument the candidates of one group give, `None` to go on with the next group.
    fn in_group(&self, t: &mut Typer<'_>, group: Vec<Candidate>) -> Result<Option<TNodeId>, &'static str> {
        let (pt, coherent) = (self.pt, self.coherent);
        let group = fitting(t, &group, |t, c| self.tried(t, c, true))?;
        let existence = self.ctx.mode.is(Mode::EXISTENCE);
        if coherent && existence && !t.types().is_fully_defined(pt) {
            return self.first_fitting(t, &group);
        }
        let unpruned = coherent || has_unbounded_var(&mut t.types(), pt);
        match self.best_first(t, &group, unpruned)? {
            BestFirst::Found(tree) => return Ok(Some(tree)),
            BestFirst::Failed if group.len() == 1 => return Ok(None),
            _ => {}
        }
        let mut nested_failure = false;
        let found = fitting(t, &group, |t, c| {
            let tried = self.tried(t, c, false);
            nested_failure |= tried == Err("no implicit argument");
            tried
        })?;
        let winner = match found.as_slice() {
            [] => return Ok(None),
            [_] if nested_failure && has_unbounded_var(&mut t.types(), pt) => {
                return Err("implicit candidate rejected by a search with type variables");
            }
            [c] => *c,
            [c, ..] if coherent => {
                if !existence && (!t.types().is_fully_defined(pt) || !self.found_one_type(t, &found)?) {
                    return Err("several CanEqual instances, tried in an order of a hash table");
                }
                *c
            }
            _ => most_specific(t, &found, self.ctx).ok_or(AMBIGUOUS)?,
        };
        self.tried(t, winner, false).map(Some)
    }
}
