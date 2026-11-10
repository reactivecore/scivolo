// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala

mod call_views;
mod conversions;
mod extension_containers;
mod extension_views;
mod fallback;
mod may_view;
mod qualifier;
mod root_members;
mod view_kinds;
mod view_scope;
mod widened;

pub use qualifier::QualifierView;

use super::super::{Pt, Typer};
use super::attempts::fitting;
use super::candidates::Candidate;
use super::ranking::{compare, most_specific};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::TypeId;
use view_kinds::{ViewKind, offers_extension, view_kind};
use widened::{beats_widened, best_widened};

/// What a conversion's result must offer.
#[derive(Clone, Copy)]
pub enum Goal<'a> {
    /// A type to conform to, as `BigInt` for `val n: BigInt = 1`.
    Conform(TypeId),
    /// A member to have, as `twice` for `"ab".twice`.
    Member(&'a str),
    /// A member applied to the arguments of a `FunProto`, by its index in `Typer::protos`.
    Call(&'a str, usize),
    /// A member whose value is expected to conform to a type.
    Typed(&'a str, TypeId),
}

impl<'a> Goal<'a> {
    /// Gives the goal of a conversion of the qualifier of a selection of `name` with expected
    /// type `pt`.
    pub fn of_select(name: &'a str, pt: Pt) -> Goal<'a> {
        match pt {
            Pt::Fun(proto) => Goal::Call(name, proto),
            Pt::Type(t) => Goal::Typed(name, t),
            _ => Goal::Member(name),
        }
    }

    /// The member the converted tree must have.
    pub fn member(self) -> Option<&'a str> {
        match self {
            Goal::Member(name) | Goal::Call(name, _) | Goal::Typed(name, _) => Some(name),
            Goal::Conform(_) => None,
        }
    }
}

/// What a view search found: a converted tree, or an implicit value with the extension method a
/// selection names.
pub enum Viewed {
    Converted(TNodeId),
    Extension(Candidate),
}

/// One search for the implicit conversion of a tree, or for the implicit value whose extension
/// method a selection calls, as `1` becomes `BigInt(1)` or `"ab".twice` calls `ops.twice("ab")`
/// (dotty's `inferView`, `tryInsertImplicitOnQualifier`): the contextual implicits by nesting
/// level, then the implicit scope of the tree's type; the budget of tries stays in
/// `Typer::history`, which nested searches share.
#[derive(Clone, Copy)]
pub struct ViewSearch<'a> {
    /// The tree to convert.
    tree: TNodeId,
    goal: Goal<'a>,
    ctx: Ctx,
}

impl Typer<'_> {
    /// Converts `tree` to fit `goal` by the implicit conversion the search selects, as `1` to
    /// `BigInt(1)` for an expected `BigInt`; `None` if none applies.
    pub fn converted(
        &mut self,
        tree: TNodeId,
        goal: Goal,
        ctx: Ctx,
    ) -> Result<Option<TNodeId>, &'static str> {
        match self.viewed(tree, goal, ctx)? {
            Some(Viewed::Converted(t)) => Ok(Some(t)),
            Some(Viewed::Extension(_)) => Err("extension method of an implicit value"),
            None => Ok(None),
        }
    }

    /// Converts `tree` to fit `goal` by the implicit conversion the search selects, or finds the
    /// implicit value with the extension method a member goal names; `None` if neither applies.
    pub fn viewed(&mut self, tree: TNodeId, goal: Goal, ctx: Ctx) -> Result<Option<Viewed>, &'static str> {
        ViewSearch::new(tree, goal, ctx).run(self)
    }
}

impl<'a> ViewSearch<'a> {
    fn new(tree: TNodeId, goal: Goal<'a>, ctx: Ctx) -> Self {
        ViewSearch { tree, goal, ctx }
    }

    /// Finds the conversion or extension container of the first group of candidates that gives
    /// one; within a group the only one, else the most specific, a conversion of the constant
    /// tree's own type above one of its widened type.
    fn run(self, t: &mut Typer<'_>) -> Result<Option<Viewed>, &'static str> {
        let (goal, ctx) = (self.goal, self.ctx);
        let from = t.zonk(t.tpd.tpe(self.tree));
        if let Goal::Conform(pt) = goal
            && (t.types().is_top_like(pt) || t.types().is_underspecified_arg(from))
        {
            return Ok(None);
        }
        if t.history.open.is_empty() {
            t.history.budget = super::search::MAX_TRIES;
        }
        let contextual = t.contextual_implicits(ctx, &mut |t, c| {
            view_kind(t, c) != ViewKind::None
                || goal.member().is_some_and(|name| offers_extension(t, c, name).unwrap_or(false))
        })?;
        let levels = contextual.len();
        let mut contextual = contextual.into_iter();
        let constant = t.constant_of_type(from).is_some();
        for _ in 0..=levels {
            let (mut group, containers) = match contextual.next() {
                Some(group) => (group, Vec::new()),
                None => {
                    let scope = self.scope(t, from)?;
                    (scope.implicits, scope.containers)
                }
            };
            let mut found = fitting(t, &group, |t, c| self.try_conversion(t, c, false))?;
            let mut widened = Vec::new();
            if constant {
                widened = fitting(t, &group, |t, c| self.try_conversion(t, c, true))?;
                widened.retain(|c| !found.contains(c));
            }
            if let Goal::Call(name, proto) = goal
                && found.len() > 1
                && most_specific(t, &found, ctx).is_none()
            {
                found = self.applying(t, found, name, proto);
            }
            if goal.member().is_some() {
                let extra: Vec<Candidate> = containers.into_iter().filter(|c| !group.contains(c)).collect();
                group.extend(extra);
                if let Some(c) = self.extension_container(t, &group)? {
                    found.retain(|&v| compare(t, v, c, ctx) != Some(-1));
                    widened.retain(|&v| compare(t, v, c, ctx) != Some(-1));
                    if found.is_empty() && widened.is_empty() {
                        return Ok(Some(Viewed::Extension(c)));
                    }
                    if !found.iter().chain(&widened).any(|&v| compare(t, v, c, ctx) == Some(1)) {
                        return Err("implicit conversion beside an extension method of an implicit value");
                    }
                }
            }
            let ambiguous = "ambiguous implicit conversions";
            let winner = match (&found[..], &widened[..]) {
                ([], []) => continue,
                ([c], _) | ([], [c]) => *c,
                ([], _) => best_widened(t, &widened).ok_or(ambiguous)?,
                _ => most_specific(t, &found, ctx).ok_or(ambiguous)?,
            };
            if found.contains(&winner) && !beats_widened(t, winner, &widened) {
                return Err(ambiguous);
            }
            let converted = self.try_conversion(t, winner, widened.contains(&winner))?;
            return Ok(Some(Viewed::Converted(converted)));
        }
        Ok(None)
    }
}
