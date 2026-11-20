// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Trying the candidates of a group one by one, in dotty's order, keeping the best success
//! (dotty's `rank`, `healAmbiguous`, `compareEligibles`).

use super::super::Typer;
use super::attempts::complete_candidate;
use super::candidates::Candidate;
use super::history::STALE;
use super::ranking::compare;
use super::search::{AMBIGUOUS, ImplicitSearch, is_misfit};
use crate::tpd::TNodeId;
use std::cmp::Reverse;
use tasty::SymbolId;

/// The refusal of a ranking that depends on a comparison of candidates the typer cannot decide.
const UNDECIDED: &str = "implicit candidates whose ranking is undecided";

/// What `attempt` on one candidate gave.
pub enum Trial {
    /// It succeeded; `linked` if it changed the dictionary.
    Fits {
        linked: bool,
    },
    Misfit,
    /// A nested search found ambiguous candidates.
    Ambiguous,
}

/// The candidates of `group` that `attempt` accepts, each tried with the constraint, the
/// recorded fallback and the dictionary restored afterwards; `Err` if one fails other than by
/// not fitting, or if the dictionary depends on which of several candidates dotty tries.
pub fn fitting<'a>(
    t: &mut Typer<'a>,
    group: &[Candidate],
    mut attempt: impl FnMut(&mut Typer<'a>, Candidate) -> Result<TNodeId, &'static str>,
) -> Result<Vec<Candidate>, &'static str> {
    let mut found = Vec::new();
    let mut linked_any = false;
    for &c in group {
        match trial(t, c, &mut attempt)? {
            Trial::Fits { linked } => {
                found.push(c);
                linked_any |= linked;
            }
            Trial::Misfit => {}
            Trial::Ambiguous => return Err(AMBIGUOUS),
        }
    }
    if linked_any && found.len() > 1 {
        return Err(STALE);
    }
    Ok(found)
}

/// `attempt` on candidate `c` with the constraint, the recorded fallback and the dictionary
/// restored afterwards; `Err` if it fails other than by not fitting or by a nested ambiguity, or
/// fails having changed the dictionary.
fn trial<'a>(
    t: &mut Typer<'a>,
    c: Candidate,
    attempt: &mut impl FnMut(&mut Typer<'a>, Candidate) -> Result<TNodeId, &'static str>,
) -> Result<Trial, &'static str> {
    complete_candidate(t, c)?;
    let (result, clean, linked) = t.speculatively(|t| attempt(t, c));
    match result {
        _ if !clean => Err(super::attempts::CANDIDATE_OUTSIDE),
        Ok(_) => Ok(Trial::Fits { linked }),
        Err(what) if (is_misfit(what) || what == AMBIGUOUS) && linked => Err(STALE),
        Err(what) if is_misfit(what) => Ok(Trial::Misfit),
        Err(AMBIGUOUS) => Ok(Trial::Ambiguous),
        Err(what) => Err(what),
    }
}

/// The successes of trying the candidates of a group in dotty's order.
pub struct Ranked {
    /// The success that ranks above the others tried, if any.
    pub winner: Option<Candidate>,
    /// How many candidates succeeded.
    pub successes: usize,
    /// Whether a candidate failed for want of a nested implicit argument.
    pub nested_failure: bool,
}

impl ImplicitSearch {
    /// Tries the candidates of `group` in dotty's order (`compareEligibles`): after a success,
    /// only those not ranking below it; after an ambiguity, of two successes or in a nested
    /// search of one, only those ranking above the ambiguous ones. `Err(AMBIGUOUS)` if an
    /// ambiguity remains, its two successes recorded if the search is outermost (dotty's `rank`);
    /// `Err(UNDECIDED)` if the outcome depends on a comparison the typer cannot decide.
    pub fn ranked(&self, t: &mut Typer<'_>, group: &[Candidate]) -> Result<Ranked, &'static str> {
        let mut pending = in_eligible_order(t, group);
        let (mut winner, mut successes, mut linked_any) = (None, 0, false);
        let mut first_ambiguity: Option<Option<(Candidate, Candidate)>> = None;
        let (mut nested_failure, mut doubtful) = (false, Vec::new());
        let mut attempt = |t: &mut Typer<'_>, c| {
            let tried = self.tried(t, c, false);
            nested_failure |= tried == Err("no implicit argument");
            tried
        };
        while !pending.is_empty() {
            let c = pending.remove(0);
            let trial = trial(t, c, &mut attempt)?;
            if !matches!(trial, Trial::Misfit) && doubtful.contains(&c) {
                return Err(UNDECIDED);
            }
            let ambiguous = match trial {
                Trial::Misfit => continue,
                Trial::Ambiguous => {
                    first_ambiguity.get_or_insert(None);
                    vec![c]
                }
                Trial::Fits { linked } => {
                    linked_any |= linked;
                    successes += 1;
                    let rank = match winner {
                        Some(r) if r == c => Some(1),
                        Some(r) => Some(ranks(t, self, r, c).ok_or(UNDECIDED)?),
                        None => None,
                    };
                    match (winner, rank) {
                        (Some(_), Some(1..)) => continue,
                        (Some(r), Some(0)) => {
                            first_ambiguity.get_or_insert(Some((r, c)));
                            vec![r, c]
                        }
                        _ => {
                            winner = Some(c);
                            pending.retain(|&d| kept(t, self, d, &[c], 0, &mut doubtful));
                            continue;
                        }
                    }
                }
            };
            winner = None;
            pending.retain(|&d| kept(t, self, d, &ambiguous, 1, &mut doubtful));
        }
        if linked_any && successes > 1 {
            return Err(STALE);
        }
        if let (None, Some(pair)) = (winner, first_ambiguity) {
            if t.history.open.is_empty() {
                t.history.ambiguity = pair;
            }
            return Err(AMBIGUOUS);
        }
        Ok(Ranked { winner, successes, nested_failure })
    }
}

/// Whether pending candidate `d` ranks at least `least` against each of `others`; one whose
/// comparison is undecided but not decided below is kept and added to `doubtful`.
fn kept(
    t: &mut Typer<'_>,
    s: &ImplicitSearch,
    d: Candidate,
    others: &[Candidate],
    least: i32,
    doubtful: &mut Vec<Candidate>,
) -> bool {
    let ranks: Vec<Option<i32>> = others.iter().map(|&o| ranks(t, s, d, o)).collect();
    let kept = ranks.iter().flatten().all(|&r| r >= least);
    if kept && ranks.contains(&None) {
        doubtful.push(d);
    }
    kept
}

/// 1 if `a` ranks above `b`, -1 if below, 0 if neither; `None` if undecided.
fn ranks(t: &mut Typer<'_>, s: &ImplicitSearch, a: Candidate, b: Candidate) -> Option<i32> {
    if a == b { Some(0) } else { compare(t, a, b, s.ctx) }
}

/// `group` sorted as dotty tries its eligible candidates: those with fewer parameters in their
/// first clause first, then those whose owner has more base classes.
fn in_eligible_order(t: &mut Typer<'_>, group: &[Candidate]) -> Vec<Candidate> {
    let mut keyed: Vec<(usize, Reverse<usize>, Candidate)> = group
        .iter()
        .map(|&c| {
            let owner = t.u.symbol(c.sym).owner;
            (first_arity(t, c), Reverse(owner.map_or(0, |o| base_classes(t, o))), c)
        })
        .collect();
    keyed.sort_by_key(|&(arity, bases, _)| (arity, bases));
    keyed.into_iter().map(|(_, _, c)| c).collect()
}

/// How many parameters the first clause of `c`'s type has (dotty's `firstParamTypes`).
fn first_arity(t: &mut Typer<'_>, c: Candidate) -> usize {
    let info = t.u.info(c.sym);
    let info = t.u.poly(info).map_or(info, |p| p.result);
    t.u.method(info).map_or(0, |m| m.params.len())
}

/// How many base classes class `c` has, an object counting its companion class's if more
/// (dotty's `compareBaseClassesLength`).
fn base_classes(t: &mut Typer<'_>, c: SymbolId) -> usize {
    if !t.u.is_class(c) {
        return 0;
    }
    let own = t.u.linearization(c).len();
    match t.u.is_module_class(c).then(|| t.u.companion_class(c)).flatten() {
        Some(k) => own.max(t.u.linearization(k).len()),
        None => own,
    }
}
