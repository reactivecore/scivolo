// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Conversion candidates in the search history, with the prototype they are tried for (dotty's
//! `SearchHistory.nest` of a view).

use super::super::super::{Pt, Typer};
use super::super::candidates::Candidate;
use super::super::history::Open;
use super::Goal;
use tasty::{Type, TypeId};

/// The types a conversion's member prototype counts in the divergence check (dotty's
/// `SelectionProto.fold`).
#[derive(Clone)]
pub struct ViewProto {
    /// The types of the arguments and of the expected result of the applied member.
    pub parts: Vec<TypeId>,
    /// Whether `parts` holds all of them.
    pub complete: bool,
}

impl Typer<'_> {
    /// `f` run with conversion candidate `c` open for `goal`. A member selected without an
    /// applicable alternative counts nothing: its expected type is ignored (dotty's `IgnoredProto`).
    pub fn viewing<T>(&mut self, c: Candidate, goal: Goal, f: impl FnOnce(&mut Self) -> T) -> T {
        let wildcard = self.u.intern_type(Type::Wildcard(None));
        let ignored = || Some(ViewProto { parts: Vec::new(), complete: true });
        let (pt, proto) = match goal {
            Goal::Conform(pt) => (pt, None),
            Goal::Member(_) | Goal::Typed(..) | Goal::Call(..) => (wildcard, ignored()),
            Goal::Applied(_, proto) => (wildcard, Some(proto.clone())),
        };
        self.history.open.push(Open { cand: c, pt, by_name: false, view: true, proto, measure: None });
        let result = f(self);
        self.history.open.pop();
        result
    }

    /// The prototype of a member applied as call `proto`, whose arguments have types `args` if
    /// they are all of its arguments (dotty's `FunProto.fold`).
    pub fn applied_view_proto(&self, proto: usize, args: Vec<TypeId>, all_args: bool) -> ViewProto {
        let (mut parts, mut complete) = (args, all_args);
        match self.protos[proto].pt {
            Pt::Any => {}
            Pt::Type(r) => parts.push(r),
            _ => complete = false,
        }
        ViewProto { parts, complete }
    }
}
