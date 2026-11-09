// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! The implicit searches in progress and the dictionary of by-name implicits (dotty's
//! `SearchHistory`, `OpenSearch`, `SearchRoot`, `recursiveRef`).

use super::super::Typer;
use super::candidates::{Candidate, is_implicit};
use super::measures::Measure;
use super::prefilter::cannot_fit;
use super::search::ImplicitSearch;
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
    /// How many more candidates the outermost search in progress may try.
    pub budget: u32,
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
    /// `f` run with conversion candidate `c` open.
    pub fn viewing<T>(&mut self, c: Candidate, f: impl FnOnce(&mut Self) -> T) -> T {
        let pt = self.u.intern_type(Type::Wildcard(None));
        self.history.open.push(Open { cand: c, pt, by_name: false, view: true, measure: None });
        let result = f(self);
        self.history.open.pop();
        result
    }
}

/// The implicit argument of an outermost search `found`, with its dictionary of by-name
/// implicits (dotty's `emitDictionary`).
pub fn emitted(
    t: &mut Typer<'_>,
    found: Result<TNodeId, &'static str>,
    at: Span,
    ctx: Ctx,
) -> Result<TNodeId, &'static str> {
    match found {
        Ok(tree) => t.with_dictionary(tree, at, ctx),
        Err(what) if t.history.lazies.is_empty() => Err(what),
        Err(_) => {
            t.history.lazies.clear();
            Err(STALE)
        }
    }
}

impl ImplicitSearch {
    /// Candidate `c` tried within the history, unless it diverges; a successful result defines
    /// the by-name implicit of `pt` if there is one (dotty's `tryImplicit`). If `shallow`, only
    /// whether its type fits, without its implicit arguments.
    pub fn tried(&self, t: &mut Typer<'_>, c: Candidate, shallow: bool) -> Result<TNodeId, &'static str> {
        let (pt, by_name) = (self.pt, self.by_name);
        t.complete(c.sym);
        if !is_implicit(t.u, c.sym) {
            return Err("not an implicit value");
        }
        let info = t.u.info(c.sym);
        let result = t.final_result(info);
        if let Some(k) = t.u.class_symbol(result) {
            t.complete(k);
        }
        if cannot_fit(t, c.sym, pt) || t.is_function_type(pt) && !self.is_eligible(t, c) {
            return Err("implicit of another type");
        }
        if self.diverges(t, c)? {
            return Err("diverging implicit");
        }
        t.history.open.push(Open { cand: c, pt, by_name, view: false, measure: None });
        let tried = self.try_implicit(t, c, shallow);
        t.history.open.pop();
        let tree = tried?;
        if shallow {
            return Ok(tree);
        }
        let Some(i) = lazy_of(t, pt) else { return Ok(tree) };
        t.history.lazies[i].rhs = Some(tree);
        let span = t.tpd.span(tree);
        Ok(lazy_ref(t, i, span))
    }

    /// A reference to the by-name implicit of `pt` constructed or under construction, the knot
    /// tied if an open search for a subtype lies beyond a by-name argument (dotty's
    /// `recursiveRef`).
    pub fn recursive_ref(&self, t: &mut Typer<'_>) -> Result<Option<TNodeId>, &'static str> {
        let (pt, at) = (self.pt, self.at);
        if let Some(i) = lazy_of(t, pt) {
            return Ok(Some(lazy_ref(t, i, at.start_pos())));
        }
        if !self.by_name && !t.history.open.iter().any(|o| o.by_name) {
            return Ok(None);
        }
        let mut below = self.by_name;
        for i in (0..t.history.open.len()).rev() {
            let Open { pt: tp, by_name: tp_by_name, view, .. } = t.history.open[i];
            if !view && (below || tp_by_name) {
                let before = t.constraint.attempt();
                if t.conforms(tp, pt) {
                    return linked(t, tp, at).map(Some);
                }
                t.constraint.abandon(before);
            }
            below |= tp_by_name;
        }
        Ok(None)
    }
}

/// A reference to the by-name implicit of `tpe`, entered if new (dotty's `linkBynameImplicit`).
fn linked(t: &mut Typer<'_>, tpe: TypeId, at: Span) -> Result<TNodeId, &'static str> {
    if t.history.open.iter().any(|o| o.view) {
        return Err("by-name implicit in a conversion");
    }
    let i = match lazy_of(t, tpe) {
        Some(i) => i,
        None => {
            t.history.names += 1;
            let number = t.history.names;
            t.history.lazies.push(Lazy { tpe, number, refs: Vec::new(), rhs: None });
            t.history.lazies.len() - 1
        }
    };
    Ok(lazy_ref(t, i, at.start_pos()))
}

/// The index of the by-name implicit of type `tpe`.
fn lazy_of(t: &mut Typer<'_>, tpe: TypeId) -> Option<usize> {
    let tpe = t.zonk(tpe);
    (0..t.history.lazies.len()).find(|&i| {
        let other = t.history.lazies[i].tpe;
        t.zonk(other) == tpe
    })
}

/// A placeholder reference to by-name implicit `i`, which the dictionary replaces.
fn lazy_ref(t: &mut Typer<'_>, i: usize, span: Span) -> TNodeId {
    let tpe = t.history.lazies[i].tpe;
    let name = t.names.intern(&format!("$_lazy_implicit_${}", t.history.lazies[i].number));
    let name = syntax::ast::TName { name, is_type: false };
    let node = t.tpd.node(crate::tpd::Tree::Ident { name }, span, Some(tpe), None);
    t.history.lazies[i].refs.push(node);
    node
}
