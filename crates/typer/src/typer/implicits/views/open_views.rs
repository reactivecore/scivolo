// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Conversion candidates in the search history, with the prototype they are tried for (dotty's
//! `SearchHistory.nest` of a view).

use super::super::super::{Pt, Typer};
use super::super::candidates::Candidate;
use super::super::history::Open;
use super::Goal;
use crate::tpd::TNodeId;
use tasty::{Type, TypeId};

/// What a conversion searched for a member is expected to provide, as far as it is known.
#[derive(Clone)]
pub struct ViewProto {
    /// The types of the member's expected type and of the arguments typed so far.
    pub parts: Vec<TypeId>,
    /// Whether `parts` holds all of them.
    pub complete: bool,
}

impl Typer<'_> {
    /// `f` run with conversion candidate `c` open for `goal`.
    pub fn viewing<T>(&mut self, c: Candidate, goal: Goal, f: impl FnOnce(&mut Self) -> T) -> T {
        let wildcard = self.u.intern_type(Type::Wildcard(None));
        let (pt, proto) = match goal {
            Goal::Conform(pt) => (pt, None),
            Goal::Member(_) => (wildcard, Some(ViewProto { parts: Vec::new(), complete: true })),
            Goal::Typed(_, pt) => (wildcard, Some(ViewProto { parts: vec![pt], complete: true })),
            Goal::Call(_, proto) => (wildcard, Some(self.call_view_proto(proto))),
        };
        self.history.open.push(Open { cand: c, pt, by_name: false, view: true, proto, measure: None });
        let result = f(self);
        self.history.open.pop();
        result
    }

    /// The known types of the application prototype `proto`: its typed arguments and its expected
    /// result.
    fn call_view_proto(&self, proto: usize) -> ViewProto {
        let p = &self.protos[proto];
        let typed: Vec<TNodeId> = p.typed.iter().flatten().flatten().copied().collect();
        let mut parts: Vec<TypeId> = typed.iter().map(|&a| self.tpd.tpe(a)).collect();
        let mut complete = typed.len() == p.args.len();
        match p.pt {
            Pt::Any => {}
            Pt::Type(r) => parts.push(r),
            _ => complete = false,
        }
        ViewProto { parts, complete }
    }
}
