// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! The implicit searches in progress and the dictionary of by-name implicits (dotty's
//! `SearchHistory`, `OpenSearch`, `SearchRoot`, `recursiveRef`).

use super::super::Typer;
use super::candidates::Candidate;
use super::measures::Measure;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use tasty::{Type, TypeId};

/// A candidate being tried for a type, by name or not; `view` for a conversion's.
pub struct Open {
    pub cand: Candidate,
    pub pt: TypeId,
    pub by_name: bool,
    pub view: bool,
    /// `typeSize` and `coveringSet` of `pt`, once asked for.
    pub measure: Option<Measure>,
}

/// A by-name implicit under construction: its type, number, the trees that refer to it, and its
/// definition once found.
#[derive(Clone)]
pub struct Lazy {
    pub tpe: TypeId,
    pub number: u32,
    pub refs: Vec<TNodeId>,
    pub rhs: Option<TNodeId>,
}

#[derive(Default)]
pub struct History {
    pub open: Vec<Open>,
    pub lazies: Vec<Lazy>,
    /// The last `$_lazy_implicit_$N` the unit used.
    pub names: u32,
}

/// The dictionary and its name counter, to restore after an attempt.
pub type Snapshot = (Vec<Lazy>, u32);

/// The refusal of a search whose dictionary depends on attempts dotty may not make.
pub const STALE: &str = "by-name implicit of a discarded candidate";

impl History {
    pub fn snapshot(&self) -> Snapshot {
        (self.lazies.clone(), self.names)
    }

    pub fn restore(&mut self, (lazies, names): Snapshot) {
        self.lazies = lazies;
        self.names = names;
    }

    /// Whether the dictionary has implicits or definitions that `before` lacks.
    pub fn changed(&self, (before, _): &Snapshot) -> bool {
        self.lazies.len() != before.len()
            || self.lazies.iter().zip(before).any(|(a, b)| (a.tpe, a.rhs) != (b.tpe, b.rhs))
    }
}

impl Typer<'_> {
    /// The implicit argument of an outermost search `found`, with its dictionary of by-name
    /// implicits (dotty's `emitDictionary`).
    pub fn emitted(
        &mut self,
        found: Result<TNodeId, &'static str>,
        at: Span,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        match found {
            Ok(tree) => self.with_dictionary(tree, at, ctx),
            Err(what) if self.history.lazies.is_empty() => Err(what),
            Err(_) => {
                self.history.lazies.clear();
                Err(STALE)
            }
        }
    }

    /// Candidate `c` tried for `pt` within the history, unless it diverges; a successful result
    /// defines the by-name implicit of `pt` if there is one (dotty's `tryImplicit`).
    pub fn tried(
        &mut self,
        c: Candidate,
        (pt, by_name): (TypeId, bool),
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<TNodeId, &'static str> {
        self.complete(c.sym);
        if !self.is_implicit(c.sym) {
            return Err("not an implicit value");
        }
        if self.cannot_fit(c.sym, pt) || self.function_arity(pt).is_some() && !self.is_eligible(c, pt) {
            return Err("implicit of another type");
        }
        if self.diverges(c, pt, by_name)? {
            return Err("diverging implicit");
        }
        self.history.open.push(Open { cand: c, pt, by_name, view: false, measure: None });
        let tried = self.try_implicit(c, pt, at, ctx, depth);
        self.history.open.pop();
        let tree = tried?;
        let Some(i) = self.lazy_of(pt) else { return Ok(tree) };
        self.history.lazies[i].rhs = Some(tree);
        Ok(self.lazy_ref(i, self.tpd.span(tree)))
    }

    /// `f` run with conversion candidate `c` open.
    pub fn viewing<T>(&mut self, c: Candidate, f: impl FnOnce(&mut Self) -> T) -> T {
        let pt = self.u.intern_type(Type::Wildcard(None));
        self.history.open.push(Open { cand: c, pt, by_name: false, view: true, measure: None });
        let result = f(self);
        self.history.open.pop();
        result
    }

    /// A reference to the by-name implicit of `pt` constructed or under construction, the knot
    /// tied if an open search for a subtype lies beyond a by-name argument (dotty's
    /// `recursiveRef`).
    pub fn recursive_ref(
        &mut self,
        pt: TypeId,
        by_name: bool,
        at: Span,
    ) -> Result<Option<TNodeId>, &'static str> {
        if let Some(i) = self.lazy_of(pt) {
            return Ok(Some(self.lazy_ref(i, at.start_pos())));
        }
        if !by_name && !self.history.open.iter().any(|o| o.by_name) {
            return Ok(None);
        }
        let mut below = by_name;
        for i in (0..self.history.open.len()).rev() {
            let Open { pt: tp, by_name: tp_by_name, view, .. } = self.history.open[i];
            if !view && (below || tp_by_name) {
                let before = self.constraint.attempt();
                if self.conforms(tp, pt) {
                    return self.linked(tp, at).map(Some);
                }
                self.constraint.abandon(before);
            }
            below |= tp_by_name;
        }
        Ok(None)
    }

    /// A reference to the by-name implicit of `tpe`, entered if new (dotty's
    /// `linkBynameImplicit`).
    fn linked(&mut self, tpe: TypeId, at: Span) -> Result<TNodeId, &'static str> {
        if self.history.open.iter().any(|o| o.view) {
            return Err("by-name implicit in a conversion");
        }
        let i = match self.lazy_of(tpe) {
            Some(i) => i,
            None => {
                self.history.names += 1;
                let number = self.history.names;
                self.history.lazies.push(Lazy { tpe, number, refs: Vec::new(), rhs: None });
                self.history.lazies.len() - 1
            }
        };
        Ok(self.lazy_ref(i, at.start_pos()))
    }

    /// The index of the by-name implicit of type `t`.
    fn lazy_of(&mut self, t: TypeId) -> Option<usize> {
        let t = self.zonk(t);
        (0..self.history.lazies.len()).find(|&i| {
            let tpe = self.history.lazies[i].tpe;
            self.zonk(tpe) == t
        })
    }

    /// A placeholder reference to by-name implicit `i`, which the dictionary replaces.
    fn lazy_ref(&mut self, i: usize, span: Span) -> TNodeId {
        let tpe = self.history.lazies[i].tpe;
        let name = self.names.intern(&format!("$_lazy_implicit_${}", self.history.lazies[i].number));
        let name = syntax::ast::TName { name, is_type: false };
        let node = self.tpd.node(crate::tpd::Tree::Ident { name }, span, Some(tpe), None);
        self.history.lazies[i].refs.push(node);
        node
    }
}
